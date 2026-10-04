# MulchLauncher privacy policy

Last updated: 4 October 2026

MulchLauncher is a game launcher for Windows. It has no account, no sign-in, no ads, no analytics and no telemetry. It doesn't collect, sell or share personal information.

## What it reads on your PC

To list your games, MulchLauncher reads the install records other launchers keep on your PC (Steam, Epic Games, Ubisoft Connect, GOG, Xbox, Battle.net, Rockstar Games and EA): their registry entries and files, and the list of installed apps. To show when you last played, it reads Windows' Game Bar records and which programs are running. It also checks whether some chat apps (such as Discord) are installed, to show a button for them.

None of this leaves your PC.

## What it saves

Settings, play history, games you added yourself, and downloaded cover images are saved in `%LOCALAPPDATA%\MulchLauncher` (for the Microsoft Store version; Windows keeps this in the app's own storage). Uninstalling MulchLauncher removes it.

## Network requests

For games that have no cover art on your PC, MulchLauncher looks up a poster from these public sources and saves it locally, so each is only fetched once (or retried after a week if none was found):

- Steam: `steamcommunity.com` (search by game name) and `steamcdn-a.akamaihd.net` (images)
- Microsoft Store catalogue: `displaycatalog.mp.microsoft.com` (lookup by the Xbox game's package name) and the Microsoft image server it returns
- Wikidata: `www.wikidata.org` (search by game name)
- Ubisoft: `ubistatic3-a.akamaihd.net` (images)
- Wikipedia: `en.wikipedia.org` (search by game name) and `upload.wikimedia.org` (images)

These requests contain the game's name or its ID in that store, nothing else about you. Like any internet request, they reveal your IP address to the server, which handles it under its own privacy policy. No cookies or identifiers are sent.

The copy downloaded from GitHub (not the Microsoft Store version) also checks `api.github.com` for a newer release and downloads it from GitHub. The Microsoft Store version is updated by the Store instead.

Buttons in the app can open web pages or other apps (for example the project's GitHub page, or a game in its own launcher). Those open in your browser or in that app and are covered by their own privacy policies.

## Contact

Questions: https://github.com/apotenza92/mulch-launcher/issues
