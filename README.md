# cargo-mirror

This is a cargo subcommand that let you download dependencies from a mirror site when building your Rust project.

To use this, you need a mirror site that mirrors all crates on [crates.io](https://crates.io), but serving these files statically just like traditional linux package repository. This can be done by a web crawler, [crates-mirror](https://github.com/tennix/crates-mirror) is an example.

*Note*: This is only for downloading dependency crates which are on [crates.io](https://crates.io). It's not designed to download dependencies from github or other places.

## How

Assuming there's a mirror site https://mirrors.ustc.edu.cn/crates, all crates on crates.io are mirrored and stored under this url.

```sh
git clone https://github.com/tennix/cargo-mirror
cd cargo-mirror
cargo build --release

# copy cargo-mirror to $HOME/.cargo/bin
# if rust version >= 1.5, just run cargo install --path FULL_PATH_OF_CURRENT_DIRECTORY
mkdir -p $HOME/.cargo/bin
cp target/release/cargo-mirror $HOME/.cargo/bin

# set environment variable
export CRATES_MIRROR_URL=https://mirrors.ustc.edu.cn/crates

# building your awesome project
cargo new myproject
cargo mirror build # or cargo mirror run
```

*NOTE*: This subcommand detects local cache directory as `$CARGO_HOME/registry/cache/github.com-88ac128001ac3a9a`, if your cache/src/index directory's hash id is not `88ac128001ac3a9a`, you need to remove your `$CARGO_HOME/registry` directory(make sure no import things there before you delete them) and rebuilt your project to let cargo generate that directory again, if it's still not like that directory, you should update your cargo to latest by `cargo install cargo`.

I guess this problem is caused by `short_hash` incompatible implementation in newer cargo, but i didn't trace deep. So if you know why and how to solve this, please let me know.


## Why

crates.io uses AWS S3 service as storage backend. In China this cloud service is blocked by the infamous [GFW](https://zh.wikipedia.org/wiki/防火长城). So many chinese rustaceans can't download their dependencies to build Rust projects. New language learners would ask why they can't build rust project using cargo again and again, this decreases their interest. So I wrote this tool to help Chinese rustaceans to get better experience writing rust programs.

Why not just setup a real crates.io mirror site?

Well, cargo can login crates.io and publish crates, so it relies on a dynamic website. For example you can't download hyper by visiting https://crates.io/api/v1/crates/hyper/0.7.1/download. In fact the real donwload url is given by the http reponse header and the body is empty:

`curl -I https://crates.io/api/v1/crates/hyper/0.7.1/download`

So it's a bit complicated to setup a full functional mirror site, besides nearly all mirror sites are only serving files statically.
