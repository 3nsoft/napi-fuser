# NAPI fuser plugin

This package gives to Node, via NAPI, a module to FUSE-mount filesystem implementation from JavaScript Node runtime. This module acts as a proxy between FUSE in Linux system and a process in JS. On OS/kernel facing side this module uses [fuser](https://docs.rs/fuser/latest/fuser/) library (FUSE in Rust style).

## Building

To use this repo, you need [Node.js](https://nodejs.org/).

Native module code is written in [Rust](https://rust-lang.org/), and uses [NAPI-RS](https://napi.rs/).

Cross compilation uses `--cross-compile` [flag](https://napi.rs/docs/cli/build#options). `rustup` targets should be added for cross-compilation.

Rust targets should be installed, listed in `package.json`.

`npm ci` installs everything.

`npm run build-all` builds everyting.


# License

Code is provided here under GNU General Public License, version 3.
