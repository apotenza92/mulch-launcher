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

Fetched posters are cached in `data\cache\posters` in the install folder; games
with no poster anywhere are only re-checked weekly. No logins or API keys.

## Sort order and play history

Games are sorted most recently played first; never-played games follow A-Z.
Last played is the latest of every source that records it reliably on this
PC: the launcher, where it keeps that locally (Steam); Windows' Game Bar,
which notes when each program it recognises as a game last ran, whatever
started it; and MulchLauncher's own tracking for everything else (starting a
game from MulchLauncher, or any of the game's programs running from its
folder, checked every 30 seconds while MulchLauncher is open). The
grid groups games played in the last week, the last month, and everything
else. Stored in `data\history.json` in the install folder.
Hours played aren't shown: most launchers keep them only in your online
account.

## Code layout

One crate per job, each buildable and testable on its own:

| Crate | Does |
|---|---|
| `crates/core` | Shared types (`Game`, `Action`, `Launcher`) and the `Library` trait |
| `crates/steam`, `epic`, `ubisoft`, `gog`, `xbox`, `battlenet`, `rockstar`, `ea`, `manual` | One library each: detect its launcher, list installed games, launch / uninstall / "show in launcher" commands, last played if known |
| `crates/art` | Icons from executables and package logos |
| `crates/posters` | Online poster lookups |
| `crates/history` | MulchLauncher's own last-played tracking |
| the root package | The app: window, install/uninstall, settings, grid layout |

Adding a launcher means adding a crate that implements `Library` and listing
it in `src/scan.rs`. `MulchLauncher --scan steam` scans one library on its own.

## Principles

1. **Zero setup.** If a user has to configure it, it's a bug.
2. **Never assume a default path.** Every location comes from the launcher's
   own records; defaults are a last-resort fallback.
3. **Insanely fast.** No disk crawling. Scans run in parallel on every start.
4. **Show what's there; don't manage it.** No database to drift out of date.

## Install

Run `MulchLauncher.exe` from anywhere (e.g. Downloads). One small window asks
just one thing, "Add to desktop" (on by default), then Install: it installs
per-user (no admin) to `%LOCALAPPDATA%\Programs\MulchLauncher` with Start menu
(and desktop) shortcuts and an "Installed apps" entry, opens, and deletes the
downloaded file if it was in Downloads. (Windows doesn't let apps pin themselves
to the taskbar; right-click it to pin.)

Everything it saves lives in that folder's `data` subfolder: settings, play
history, games added by hand, and cached posters and icons. Uninstalling from
Windows Settings removes, in full: the folder, the Start menu and desktop shortcuts, a
taskbar pin if you made one, the Installed apps entry, Windows' own cache
entries for the program, and folders older versions used in AppData. Nothing
is left behind.

## Updates

Installed copies check GitHub for a newer release at startup and every 6
hours. A new version downloads in the background, is checked against its
SHA-256 checksum, and is swapped in for the running program. MulchLauncher
then restarts into it: straight away if you're not using it, otherwise as
soon as you switch to another window. It reopens exactly where it was
(position, size, maximised or minimised), behind the window you're in,
without taking focus.

To publish an update: bump `version` in Cargo.toml, then run
`scripts\release.ps1` (needs the GitHub CLI, signed in).
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
