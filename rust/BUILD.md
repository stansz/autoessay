# Building without approval prompts

The sandbox blocks writes to `~/.cargo`, so a plain `cargo build` needs elevated
access (an approval prompt). Keep cargo's caches inside the repo instead and
everything runs under normal workspace permissions:

```sh
cd rust
source $HOME/.cargo/env
export CARGO_HOME="$PWD/.cargo-home"
cargo build
```

Or in one line:

```sh
CARGO_HOME="$PWD/.cargo-home" cargo test --workspace
```

`.cargo-home/`, `.rustup-home/`, and `target/` are gitignored. The Rust
toolchain itself still lives in `~/.rustup` (installed once via rustup) and is
only read from, so it needs no prompt.
