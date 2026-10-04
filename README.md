# MulchLauncher

A game launcher for Windows. It shows the games installed by Steam, Epic,
Ubisoft Connect, GOG, Xbox, Battle.net, Rockstar and EA, plus any you add
yourself.

[Download](https://apotenza92.github.io/mulch-launcher/)

- Games are found from each launcher's own records and played through it.
- Games are grouped by when you last played them.
- It updates itself.
- Uninstall from Windows Settings; it removes everything it added.

## Development

```
cargo run                      # the app (dev builds keep data in target\)
cargo run -- --scan            # list detected games
cargo run -- --discover        # list games found outside launchers
cargo test --workspace
scripts\release.ps1            # publish the version in Cargo.toml
```

Each launcher is its own crate in `crates/`.
