# Microsoft Store listing: MulchLauncher

Text to paste into Partner Center. Language: English (United States).

## Product name

MulchLauncher

## Short description

One launcher for all your games.

## Description

Certification needs a real description (policy 10.1.4.3: "a few words or just the app title is not sufficient").

MulchLauncher shows all the PC games you have installed in one window, whichever launcher installed them, so you can find and start any of them from one place.

- It finds games installed by Steam, Epic Games, Ubisoft Connect, GOG, the Xbox app, Battle.net, Rockstar Games and the EA app automatically. There's nothing to set up.
- Each game starts through its own launcher, so sign-ins, updates, cloud saves and overlays work as usual.
- Games are grouped by when you last played them, most recent first.
- Every game shows its cover art, and a button opens its page in its own launcher.
- Add any other game by choosing its program.
- Light and dark themes, following Windows or your choice.
- No account and no tracking: your library stays on your PC.

MulchLauncher isn't affiliated with or endorsed by these launchers' makers; their names are trademarks of their respective owners.

## Product features

None.

## Search terms (7 max)

No other products' names (Store policy 10.1.3).

1. game launcher
2. game library
3. launcher
4. pc games
5. games
6. library
7. organiser

## Category

Listed as an **app**, category **Utilities & tools**. Certification has flipped on this under policy 10.1.21: the app entry 9NQK96N5M6PH was told to use a game category (2026-10-06), then the game entry 9N1BPQC9G6ZW was told it has no game functionality and must be an app (2026-10-07). Following the most recent instruction, the current entry is 9N541CCDWDQH (MSIX or PWA app), submitted 2026-10-07. No subcategory: Utilities + tools only offers Backup + manage and File managers.

## Notes for certification

> MulchLauncher is a utility that organises and launches games the user already has installed through other launchers. It is not a game itself. Previous reviews gave conflicting category instructions under policy 10.1.21: product 9NQK96N5M6PH (app category) was told to use a game category, and product 9N1BPQC9G6ZW (game category) was told it has no game functionality and to create a new app in an app category. This submission follows the most recent instruction and is listed as an app under Utilities & tools.

## Pricing and availability

- Price: Free
- Free trial: none
- Markets: all markets
- Visibility: public, discoverable in the Store
- In-app purchases: none

## Privacy policy URL

Required because the app connects to the internet (it downloads posters). Properties answer: "Yes, my product uses personal information".

https://github.com/apotenza92/mulch-launcher/blob/main/PRIVACY.md

## Support / website

- Website: https://github.com/apotenza92/mulch-launcher
- Support contact: https://github.com/apotenza92/mulch-launcher/issues

## Age rating (IARC questionnaire)

Suggested answers:

- Category of app: choose the non-game option ("App", or "Utility/productivity/communication/other" depending on wording). Not "Game".
- Violence, blood, fear, sexuality, nudity, language, drugs, alcohol, tobacco, gambling (real or simulated): No to all. The app itself contains none of these.
- Users can interact or exchange content (chat, messages, user-generated content shared with others): No. It has no chat or sharing. The toolbar can show buttons that open chat apps already installed (Discord, Telegram and so on), but those run as separate programs; MulchLauncher doesn't carry their content.
- Shares user's location: No.
- Shares personal information with third parties: No.
- Digital purchases / in-app purchases: No.
- Unrestricted internet access (web browser, search engine): No. It only fetches cover images from fixed sources; the user can't browse.

Submitted answers (2026-10-07, IARC 10.3): All Other App Types; Online Content Yes (downloaded box art), violence can be visually depicted, not gory, not strictly cartoony, not the app's purpose; everything else No. Result: IARC 12+, ESRB Teen, PEGI !, USK 12, Russia 18+.

Note on cover art: posters are the official box art of games the user already has installed, fetched by exact title. For mature games (e.g. GTA, Doom) that box art can show weapons or violence. This is content from the user's own library, not the app's, and IARC questions are about the app's own content, so "No" is defensible. If you want to be cautious, answer the violence question as "mild / fantasy violence in imagery" and accept a 7+/12+ rating; a higher rating than needed doesn't block anything, but a too-low one can be challenged later.

## Restricted capability: runFullTrust

Partner Center asks why the package declares `runFullTrust` (500 characters max). Paste:

> MulchLauncher is a packaged Win32 desktop app that lists and starts games installed by other launchers. It reads their install records (registry, config files under Program Files and ProgramData, installed Store packages), starts games through the launcher's link or the game's program, and checks running processes to record when a game was last played. These are outside the app container. No drivers, services or admin rights; its own data stays in package storage.

## Screenshots

Requirement for desktop: PNG, 1366x768 or larger (landscape or portrait), up to 10, max 50 MB each.

Current files:

| File | Size | Qualifies |
| --- | --- | --- |
| docs/screenshots/library.png | 968 x 791 | No (too narrow) |
| docs/screenshots/hover.png | 968 x 791 | No (too narrow) |

Neither can be used as is. Recapture with the window at least 1366 px wide (for example 1600 x 900, or a maximised window on a 1080p display). Upscaling the existing files would technically pass but looks soft; better to recapture.

Suggested set (with captions):

1. The library, full window. Caption: "Games from every launcher in one window."
2. Hovering a game, showing its buttons. Caption: "Each game starts through its own launcher."
3. Adding a game by hand. Caption: "Add any other game by choosing its program."

## Store logos (already in packaging/store/Assets)

Partner Center may also ask for a 1:1 Store display image (300 x 300 minimum) and optional 9:16 / 16:9 promotional art. `assets/brand/logo-512.png` or `logo-1024.png` can be used for the 1:1 image.
