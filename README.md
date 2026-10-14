# <a href="https://rethy.xyz/projects/treechord/">Treechord

<img src="Treechord.ico">

https://rethy.xyz/projects/treechord/
</a>

## Rust version

Same configuration file (`C:\Users\<user>\Treechord.json`) and the same keys as the AutoHotkey version.

    treechord.exe [CONFIG.json]                     run (optionally with another config file)
    treechord.exe --check [CONFIG.json]             validate and list bindings
    treechord.exe --trigger bindE [CONFIG.json]     run one binding and exit

Win+Ctrl+R (or the tray menu) reloads the configuration.

Build with Visual Studio Build Tools installed: `cargo build --release`. Releases are built by `.github/workflows/release.yml` when a GitHub release is published.
Without them (GNU toolchain, rust-lld):

    rustup toolchain install stable-x86_64-pc-windows-gnu --profile minimal
    set RUSTFLAGS=-Clinker=rust-lld -Clinker-flavor=ld.lld -Clink-self-contained=yes
    cargo +stable-x86_64-pc-windows-gnu build --release --target-dir target-gnu
