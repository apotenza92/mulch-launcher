# Mulch

**The low-maintenance game launcher.**

Gardeners lay mulch so they never have to weed. Mulch does the same for your
game library: install it, open it, and every game you already have installed,
from every launcher and every drive, is just there. No accounts, no imports, no
setup, no tending.

Built for people with jobs, kids and very little time, who want their precious
few hours of gaming with their mates to *just work*.

## What it does

- **Finds every installed game instantly.** On startup Mulch reads each
  launcher's own records (Steam library folders, Epic manifests, the Ubisoft
  and GOG registry entries, Windows' package database for Xbox games), so games
  on any drive or in any custom folder are found in milliseconds. Uninstalled
  games simply disappear and new ones appear, every time it opens.
- **One click to play.** Games start through their own launcher, so overlays,
  cloud saves and DRM keep working.
- **Your launchers, one row of buttons.** Mulch shows a button for each
  launcher it detects.
- **Add anything else.** Point Mulch at any executable to add it.
- **Leaves the real work to the real apps.** Installing, updating and
  uninstalling are handed to each game's own launcher.

## Supported launchers

| Launcher | Games detected from | Launch | Uninstall |
|---|---|---|---|
| Steam | `libraryfolders.vdf` + `appmanifest_*.acf` | `steam://rungameid/` | `steam://uninstall/` |
| Epic Games | launcher `.item` manifests | `com.epicgames.launcher://` | — |
| Ubisoft Connect | `Ubisoft\Launcher\Installs` registry | `uplay://launch/` | `uplay://uninstall/` |
| GOG | `GOG.com\Games` registry | GOG Galaxy or the game's exe | — |
| Xbox / Microsoft Store | Windows package database (packages with an Xbox game config and a launchable app) | `shell:AppsFolder` | — |
| Battle.net | Windows uninstall entries (`--uid=`) + known product list | `Battle.net.exe --exec="launch <code>"` | its uninstaller |
| Rockstar Games | Windows uninstall entries (`uninstall=<title>`) + known title list | `Launcher.exe -launchTitleInFolder` | `Launcher.exe -uninstall=` |
| EA app | launcher button only for now: every known method of finding EA's installed games needs an EA login | | |

Detection rules for Battle.net, Rockstar, Epic filtering and Ubisoft links follow
[Playnite's library extensions](https://github.com/JosefNemec/PlayniteExtensions).

## Artwork

Posters are always shown whole, never cropped or stretched.

- Steam: its own local library art.
- Xbox / Microsoft Store: the "Poster" image from Microsoft's public store
  catalogue.
- Everything else: Steam's poster for a game with exactly the same name;
  for Ubisoft, Ubisoft's own thumbnail if the game isn't on Steam.
- Otherwise the game's icon.

Fetched posters are cached in `%LOCALAPPDATA%\MulchLauncher\posters`; games
with no poster anywhere are only re-checked weekly. No logins or API keys.

## Principles

1. **Zero setup.** If a user has to configure it, it's a bug.
2. **Never assume a default path.** Every location comes from the launcher's
   own records; defaults are a last-resort fallback.
3. **Insanely fast.** No disk crawling. Scans run in parallel on every start.
4. **Show what's there; don't manage it.** No database to drift out of date.

## Install

Run `MulchLauncher.exe` from anywhere (e.g. Downloads). First-run setup shows
what it found, offers to add other games, and an optional taskbar pin, then
installs itself per-user (no admin) to `%LOCALAPPDATA%\Programs\MulchLauncher`
with a Start menu shortcut and an "Installed apps" entry. Uninstall from
Windows Settings like any other app.

## Roadmap

See [docs/nudge-updates.md](docs/nudge-updates.md) for the nudge-updates plan.


- EA app game detection without a login
- Nudge updates: update one game, or everything, before game night,
  including remotely from your phone (QR-paired, end-to-end encrypted, nothing
  to host), keeping the PC awake until downloads finish
- One download priority list across all launchers
- Group game nights: everyone's PC pre-updates the game you're all playing
- Installer: initial scan, offer to add manual games, optional taskbar pin

## Development

Requires Rust (MSVC toolchain) on Windows. Release builds also need `fxc.exe`
from the Windows SDK.

```bash
cargo run              # the app
cargo run -- --scan    # print every detected game and scan timings (MulchLauncher.exe --scan)
cargo test
```
