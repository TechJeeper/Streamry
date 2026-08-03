# Streamry

Easy, local Twitch bot for streamers — commands, timers, giveaways, and automations.

**Website:** [techjeeper.github.io/Streamry](https://techjeeper.github.io/Streamry/)  
**Source:** [github.com/TechJeeper/Streamry](https://github.com/TechJeeper/Streamry)

## Download

| Platform | Installer |
| -------- | --------- |
| Windows 10/11 (x64) | [Streamry_0.1.0_x64-setup.exe](Website/downloads/Streamry_0.1.0_x64-setup.exe) |
| macOS (Apple silicon + Intel) | [Streamry_0.1.0_universal.dmg](Website/downloads/Streamry_0.1.0_universal.dmg) |

Or use the [downloads page](https://techjeeper.github.io/Streamry/downloads.html).

## Features

- Connect as your streamer account or a dedicated bot account
- Chat commands, timers, giveaways (CSPRNG winner draws), event automations
- Import commands/timers from StreamElements export ZIPs
- Minimize to tray, start on boot, backup/restore (`.streamry`)

## Requirements (build from source)

- Node.js 20+
- Rust (rustup)
- OS build tools (Xcode CLT on macOS, VS Build Tools on Windows)

## Twitch app (required once)

1. Open [Twitch Developer Console](https://dev.twitch.tv/console)
2. Create an application with client type **Public**
3. Copy the **Client ID** into Streamry’s setup wizard

Device Code login does **not** need a client secret.

## Develop

```bash
npm install
npm run tauri dev
```

## Build installers

**Windows (local):**

```bash
npm run build:win
```

**macOS (remote Mac via SSH):**

```bash
npm run build:mac:remote
```

Uses `cody@192.168.68.92` by default (`MAC_BUILD_SSH_HOST` / `.macssh` to override). Artifacts land in `dist-packages/`.

**Current machine only:**

```bash
npm run tauri build
```

Artifacts land under `src-tauri/target/release/bundle/` (DMG / NSIS / AppImage / deb depending on OS).

## Website / GitHub Pages

The marketing site lives in [`Website/`](Website/) and deploys via [`.github/workflows/pages.yml`](.github/workflows/pages.yml).

1. Repo **Settings → Pages → Source:** GitHub Actions
2. Push to `main` (or run the workflow manually)
3. Site: `https://techjeeper.github.io/Streamry/`
