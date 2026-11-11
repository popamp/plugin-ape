# Monkey's Audio for Popamp

A [Popamp](https://github.com/popamp/popamp) decoder plug-in: it plays
Monkey's Audio (`.ape`) files, the lossless format, with their APEv2
tags. Decoding is [ape-decoder](https://crates.io/crates/ape-decoder),
in pure Rust, so the plug-in is one WebAssembly module with nothing to
install beside it.

It is also the example of a decoder plug-in: `src/track.rs` is the
decoding, tested natively against reference WAVs, `src/input.rs` reads
the file from the player piece by piece, and `src/lib.rs` has the calls
`open_file`, `decode` and `seek`, described in
[popamp-plugin-api](https://github.com/popamp/popamp-plugin-api) (API
1.1). It needs no permissions: it reads the file it plays and nothing
else.

## Install

You need Popamp Plus, the build with plug-ins, 0.9.0 or newer. From
[apt.popamp.app](https://apt.popamp.app):

```sh
sudo apt install popamp-plugin-ape
```

It stays off until you allow it, as below. Or, with any build with
plug-ins (`cargo build --release --features plugins`), clone it into
Popamp's plug-ins folder:

```sh
git clone https://github.com/popamp/plugin-ape ~/.local/share/popamp/plugins/ape
```

The folder must be called `ape`, the plug-in's id. Then in Popamp:
Options > Plug-ins…, tick Monkey's Audio and press OK. `.ape` files then
open like any other, in folders, playlists and CUE sheets.

To update: `git pull` in that folder.

## What it plays

8, 16, 24 and 32-bit files, mono or stereo, of Monkey's Audio 3.95 and
newer. A file with more than two channels plays its first two. Seeking
lands on the exact sample.

## Build

```sh
rustup target add wasm32-unknown-unknown   # once
./build.sh                                 # writes plugin.wasm
cargo test                                 # the decoding, natively
```

`plugin.wasm` is committed, so cloning the repository installs a working
plug-in; rebuild it with `./build.sh` whenever `src/` changes.

The test files in `tests/fixtures` come from ape-decoder's repository
(MIT, see `tests/fixtures/LICENSE-MIT.ape-decoder`): `.ape` files and
the WAVs the reference decoder made of them.

## License

MIT or Apache-2.0, at your option.
