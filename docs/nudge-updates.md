# Nudge updates — plan (not implemented yet)

**Goal:** you get one free evening. When you sit down, the game is already
updated. A "nudge" tells your PC to update one game, or every game, ahead of
time. You can send it from the PC itself or, more usefully, from your phone
at lunchtime.

## User stories

1. *"Update Helldivers before tonight."* From my phone, I tap the game, and my
   PC updates it while I'm at work.
2. *"Update everything."* One button, every platform.
3. *"I want this one first."* One priority list across Steam, Epic, Xbox and
   the rest.
4. *"Is it ready?"* My phone shows Ready, Updating 63% or Waiting.
5. *Later:* group game nights. Everyone's PC pre-updates the game the group is
   playing.

## Building blocks

### 1. Knowing whether a game needs an update

| Platform | Local signal | Confidence |
|---|---|---|
| Steam | `appmanifest_<id>.acf`: `StateFlags` bits (2 = update required, 4 = fully installed, 1024 = update started, 1048576 = downloading) plus `BytesToDownload` / `BytesDownloaded` for progress | High (well known; verify bit values in the prototype) |
| Xbox / Store | `Windows.ApplicationModel.Store.Preview.InstallControl.AppInstallManager`: `SearchForUpdatesAsync(productId)` and `AppInstallItem` progress | Medium. It may need capabilities an unpackaged app doesn't have; prototype first |
| Epic | Manifest `AppVersionString` compared with the launcher's catalogue cache | Low. Likely "launch Epic and let it update" |
| Ubisoft, EA, Battle.net, Rockstar | No reliable local signal found yet | Low. Best effort (see below) |

### 2. Triggering the update

- **Steam:**
  - Make sure Steam is running (silently: `steam.exe -silent`).
  - Set the game's `AutoUpdateBehavior` to "high priority" in its `.acf`
    (written while Steam isn't running, or Steam overwrites it).
  - Steam then schedules the update immediately.
  - Lower-priority games get "only update when launched" for the duration, so
    the top game gets the bandwidth. Restore the user's original values
    afterwards (keep a backup of what we changed).
  - Steam's own drag-to-reorder queue has no public API. Its internal
    JavaScript (`SteamClient.Downloads`, used by Millennium and Decky) breaks
    with Steam updates, so don't build on it.
- **Xbox / Store:** `AppInstallManager.StartProductInstallAsync` /
  `UpdateAppByPackageFamilyNameAsync` if the prototype shows we're allowed.
  Otherwise open the Store's Downloads page (`ms-windows-store://downloadsandupdates`).
- **Everyone else:** start the launcher minimised. Ubisoft, EA, Battle.net and
  Epic all auto-update installed games while running. For "is it done?",
  watch the game folder's size and launcher process network activity until
  they're quiet for N minutes. Label these "best effort" in the UI.

### 3. Cross-platform priority queue

- A single ordered list in MulchLauncher: drag games up and down.
- Only one platform downloads at a time. Work through the list top to bottom,
  starting the next platform when the previous finishes (or after a timeout),
  so the top game is never fighting another launcher for bandwidth.
- Within Steam, enforce order with `AutoUpdateBehavior` as above.

### 4. Keeping the PC awake

- While a nudge is running: `SetThreadExecutionState(ES_CONTINUOUS |
  ES_SYSTEM_REQUIRED)`, released as soon as the queue finishes. Never keep the
  PC awake indefinitely.
- If the PC is asleep, a phone message can't wake it. Use Windows **wake
  timers** instead: the user picks check-in times (for example weekdays
  5pm). Mulch registers a waitable timer with resume
  (`CreateWaitableTimer` + `SetWaitableTimer(..., fResume = TRUE)`), or a
  Task Scheduler task with "Wake the computer to run this task".
- On wake: fetch pending nudges, run them, then let Windows sleep again.
- Setup checks that wake timers are allowed (`powercfg /waketimers`, plus the
  power plan's "Allow wake timers") and offers to fix that.
- **Optional, advanced:** Wake-on-LAN from another always-on device on the same
  network (router, NAS, Pi). It's not needed for the basic feature.

### 5. Phone → PC with nothing to host

- **Relay:** [ntfy](https://ntfy.sh). It's free, open source and HTTP-based.
  - The PC subscribes to a random, unguessable topic.
  - The phone publishes to it.
  - ntfy caches messages (12 hours by default), so a nudge sent while the PC
    sleeps is picked up on the next wake.
  - Self-hostable later if needed.
- **Remote:** a static web page, served free from GitHub Pages, so there's no
  server of ours. The user opens it once on the phone and taps "Add to Home
  Screen", and it behaves like an app.
- **Pairing:** MulchLauncher shows a QR code. Scanning it opens the remote
  with the topic and a 256-bit key in the URL *fragment* (`#...`), which never
  leaves the phone.
- **Privacy:** everything is end-to-end encrypted with that key
  (XChaCha20-Poly1305), so ntfy only ever sees ciphertext.
  - The PC publishes its game list and live status on a second topic.
  - The phone shows your real games and progress.
- **Commands:** `nudge {game_id}`, `nudge_all`, `set_priority [...]`,
  `cancel`. Each carries a timestamp and nonce, and the PC rejects replays.
- **Rejected alternatives:**
  - Email/IMAP: OAuth setup is painful.
  - WhatsApp: needs a Business API account and a server.
  - SMS: costs money.
  - Discord/Telegram bots: each user would need their own bot token.

### 6. Group game nights (later)

- A group is just a shared ntfy topic plus a key, joined by QR code or link.
- "Game night: Helldivers 2, Friday 8pm" makes every member's Mulch queue that
  game, if it's installed, for its next wake/check-in before 8pm. Each PC
  reports Ready or Updating back to the group.

## Milestones

1. **Steam nudge, local.**
   - "Update now" and "Update all" in the tile menu.
   - Keep the PC awake until the downloads finish.
   - Progress shown from the `.acf` files.
2. **Priority queue** across Steam (`AutoUpdateBehavior`) and best-effort
   launchers.
3. **Phone remote:** QR pairing, ntfy relay, encrypted messages, static PWA.
4. **Wake timers:** scheduled check-ins, plus the setup step to allow wake
   timers.
5. **Xbox / Store:** after the `AppInstallManager` prototype.
6. **Group game nights.**

## Open questions / prototypes first

- Steam: does changing `AutoUpdateBehavior` while Steam is closed reliably
  make it start updating that game on launch? Is a Steam restart acceptable
  if Steam was already running?
- Store: which `AppInstallManager` calls work from an unpackaged app?
- Does ntfy.sh's free tier rate-limit a few dozen messages a day per user?
  (Expected fine.)
- Battery laptops: never wake on battery, only on AC.
