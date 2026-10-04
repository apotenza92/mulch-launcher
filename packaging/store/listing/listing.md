# Microsoft Store listing: MulchLauncher

Text to paste into Partner Center. Language: English (United States).

## Product name

MulchLauncher

## Short description

A game launcher that automatically finds all your games, from every launcher.

## Description

A game launcher that automatically finds all your games, from every launcher.

## Product features

None.

## Search terms (7 max)

1. game launcher
2. game library
3. steam
4. epic games
5. gog
6. xbox
7. battle.net

(If Partner Center objects to third-party names as search terms, swap 3-7 for: games, library, launcher, pc games, organiser.)

## Category

Recommended: **Utilities & tools** (subcategory: none, or "Other" if required).

Reason: MulchLauncher isn't a game, it's a tool for starting games you already have. Listing it under Games puts it among games, gives it game-specific requirements in certification, and invites the review question of what game is being sold. Utilities & tools describes what it does.

Alternative if you'd rather it be found by people browsing games: Entertainment. Avoid Games > any subcategory.

## Pricing and availability

- Price: Free
- Free trial: none
- Markets: all markets
- Visibility: public, discoverable in the Store
- In-app purchases: none

## Privacy policy URL

Required because the app connects to the internet (it downloads posters).

Text: `packaging/store/listing/privacy.md`.

Proposed URL (publish privacy.md there as an HTML page on the existing GitHub Pages site before submitting):

https://apotenza92.github.io/mulch-launcher/privacy.html

## Support / website

- Website: https://apotenza92.github.io/mulch-launcher/
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

Expected result: the lowest rating (e.g. ESRB Everyone / PEGI 3 / IARC 3+).

Note on cover art: posters are the official box art of games the user already has installed, fetched by exact title. For mature games (e.g. GTA, Doom) that box art can show weapons or violence. This is content from the user's own library, not the app's, and IARC questions are about the app's own content, so "No" is defensible. If you want to be cautious, answer the violence question as "mild / fantasy violence in imagery" and accept a 7+/12+ rating; a higher rating than needed doesn't block anything, but a too-low one can be challenged later.

## Restricted capability: runFullTrust

Partner Center asks why the package declares `runFullTrust`. Paste:

> MulchLauncher is a Win32 desktop app (packaged with the Desktop Bridge) and needs runFullTrust to run as one. Its purpose is to list and start games installed by other launchers (Steam, Epic Games, Ubisoft Connect, GOG, Xbox, Battle.net, Rockstar Games and EA). To do this it reads those launchers' install records on the user's PC: their registry entries, manifest and configuration files under Program Files and ProgramData, and the list of installed Microsoft Store/Xbox packages. It starts games by handing the launcher's link (e.g. steam://) to Windows or by running the game's program, and it reads which processes are running to record when a game was last played. These locations and actions are outside the app container, so they are not possible from a sandboxed app. It doesn't install drivers or services, doesn't need administrator rights, and keeps its own data in its package storage.

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
