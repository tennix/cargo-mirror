extern crate curl;
extern crate toml;
extern crate cargo;
extern crate crypto;
extern crate rustc_serialize;
#[macro_use]
extern crate lazy_static;

use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::os::unix::fs::MetadataExt;
use std::process::Command;
use std::io::{self, Read, BufRead, BufReader, Write};

use curl::http;
use toml::Value;
use cargo::core::source;
use cargo::util::hex;
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use rustc_serialize::json::Json;

static OFFICIAL_REPO: &'static str = "registry+https://github.com/rust-lang/crates.io-index";

lazy_static!{
    static ref HASH: String = {
        let sid = source::SourceId::from_url(OFFICIAL_REPO.to_string());
        hex::short_hash(&sid)
    };
    static ref CARGO_HOME: String = match env::var("CARGO_HOME") {
        Ok(val) => val,
        Err(_) => env::var("HOME").expect("environment variable $HOME must be set") + "/.cargo",
    };
    static ref CARGO_MIRROR: String = match env::var("CARGO_MIRROR") {
        Ok(val) => val,
        Err(_) => "https://mirrors.ustc.edu.cn/crates".to_string(),
    };
}

#[derive(Debug)]
struct Crate<'a> {
    name: &'a str,
    version: &'a str,
    cksum: String,
    data: Option<Vec<u8>>,
    path: Option<PathBuf>,
}

impl<'a> Crate<'a> {
    fn new(name: &'a str, version: &'a str) -> Crate<'a> {
        Crate{
            name: name,
            version: version,
            cksum: String::new(),
            data: None,
            path: None,
        }
    }

    fn exists(&self) -> bool {
        let cache_exist = match self.path {
            Some(ref p) => p.exists(),
            None => false,
        };
        let src_exist = match self.src_path() {
            Some(ref p) => p.exists(),
            None => false,
        };
        cache_exist || src_exist
    }

    fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    fn save(&self) -> io::Result<()> {
        let path = self.path.clone().unwrap_or(PathBuf::new());
        let mut crate_file = try!(File::create(path.clone()));
        let data = self.data.clone();
        match data {
            Some(content) => {
                try!(crate_file.write(content.as_ref()));
            },
            None => {
                try!(fs::remove_file(path));
            }
        };
        Ok(())
    }

    fn download(&mut self) -> bool {
        let path = match self.name.len() {
            1 => format!("1/{}", self.name),
            2 => format!("2/{}", self.name),
            3 => format!("3/{}/{}", &self.name[..1], self.name),
            _ => format!("{}/{}/{}", &self.name[0..2], &self.name[2..4], self.name),
        };
        let url = format!("{}/{}/{}-{}.crate", *CARGO_MIRROR, path, self.name, self.version);
        let resp = http::handle()
            .get(&url[..])
            .exec().unwrap();
        println!("downloading {} from {}", url, *CARGO_MIRROR);
        if resp.get_code() == 200 {
            let data = resp.move_body();
            self.data = Some(data);
            return self.verify();
        } else {
            return false;
        }
    }

    fn verify(&self) -> bool {
        let mut hasher = Sha256::new();
        let data = match self.data.clone() {
            Some(data) => data,
            None => return false,
        };
        hasher.input(data.as_ref());
        hasher.result_str() == self.cksum
    }

    fn src_path(&self) -> Option<PathBuf> {
        match self.path {
            Some(ref cache_path) => Some(PathBuf::from(cache_path.to_str().unwrap().replace("cache", "src"))),
            None => None,
        }
    }
    
    fn index_path(&self) -> Option<PathBuf> {
        match self.path {
            Some(ref cache_path) => Some(PathBuf::from(cache_path.parent().unwrap().to_str().unwrap().replace("cache", "index"))),
            None => None,
        }
    }

    // use local index
    fn retrieve_checksum(&mut self) -> io::Result<()> {
        let index_path = self.index_path().unwrap_or(PathBuf::new());
        let index_file = match self.name.len() {
            1 => index_path.join("1").join(self.name),
            2 => index_path.join("2").join(self.name),
            3 => index_path.join("3").join(&self.name[..1]).join(self.name),
            _ => index_path.join(&self.name[0..2])
                .join(&self.name[2..4])
                .join(self.name),
        };
        let f = try!(File::open(index_file));
        let reader = BufReader::new(f);
        for line in reader.lines() {
            let j = match Json::from_str(line.unwrap().as_ref()) {
                Ok(jo) => jo,
                Err(_) => Json::Null,
            };
            if j.find("vers").map(|v| v.as_string()) == Some(Some(self.version)) {
                let cksum = j.find("cksum").unwrap().as_string().unwrap().to_owned();
                self.cksum = cksum;
                break;
            }
        }
        Ok(())
    }

}

impl<'a> fmt::Display for Crate<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "name: {}, version: {}, checksum: {}", self.name, self.version, self.cksum)
    }
}


fn main() {
    update_index();             // make sure index is the latest
    let cache_dir = Path::new(&*CARGO_HOME)
        .join("registry")
        .join("cache").
        join(format!("github.com-{}", *HASH));
    let dependency = read_dependency().unwrap();
    let value: Value = dependency.parse().unwrap();
    let pkgs = value.lookup("package").unwrap().as_slice().unwrap();
    let mut crates: Vec<Crate> = Vec::new();
    for pkg in pkgs {
        let src = pkg.lookup("source").unwrap().as_str().unwrap();
        if src == OFFICIAL_REPO {
            let name = pkg.lookup("name").unwrap().as_str().unwrap();
            let version = pkg.lookup("version").unwrap().as_str().unwrap();
            let mut krate = Crate::new(name, version);
            krate.set_path(cache_dir.join(format!("{}-{}.crate", name, version)));
            let _ = krate.retrieve_checksum();
            // println!("{}", krate);
            crates.push(krate);
        }
    }
    for krate in &mut crates {
        if !krate.exists() {
            if krate.download() {
                println!("download {}-{} success, {}K", krate.name, krate.version, krate.data.to_owned().unwrap().len() as f32 / 1024.0);
                let _ = krate.save();
            } else {
                println!("download {}-{} failed", krate.name, krate.version);
            }
        }
    }
    let args: Vec<String> = env::args().skip(2).collect();
    let _ = Command::new("cargo").args(&args).status();
}

fn update_index() {
    println!("Updating registry `https://github.com/rust-lang/crates.io-index`");
    Command::new("cargo")
        .arg("search cargo-mirror")
        .output()
        .expect("must connected to internet");
}

fn read_dependency() -> io::Result<String>{
    let output = Command::new("cargo")
        .arg("locate-project")
        .output()
        .expect("must in a cargo project directory");
    let res = String::from_utf8_lossy(&output.stdout).into_owned();
    let json = match Json::from_str(&*res) {
        Ok(j) => j,
        Err(_) => panic!("Couldn't parse the output of `cargo locate-project`")
    };
    let toml_file = json["root"].as_string().unwrap().to_string();
    let toml_meta = try!(fs::metadata(&toml_file));
    let lock_file = toml_file.replace("Cargo.toml", "Cargo.lock");

    if !Path::new(&lock_file).exists() || try!(fs::metadata(&lock_file)).mtime() < toml_meta.mtime(){
        Command::new("cargo").arg("generate-lockfile").output().expect("Cargo.lock must be generated");
    }

    let mut f = try!(File::open(&lock_file));
    let mut s = String::new();
    try!(f.read_to_string(&mut s));
    Ok(s)
}
