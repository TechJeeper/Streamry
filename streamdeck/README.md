# Streamry Stream Deck Plugin

Node.js plugin that talks to Streamry’s localhost control API (`127.0.0.1:1920`).

## Develop

```bash
cd streamdeck
npm install
npm run build
```

Output lands in `com.streamry.streamdeck.sdPlugin/`. Copy that folder into the Elgato Plugins directory, or use **Settings → Install StreamDeck Integration** in Streamry (uses the copy under `src-tauri/resources/streamdeck/`).

After building, sync the bundled resource:

```bash
# from repo root (PowerShell)
Remove-Item -Recurse -Force src-tauri/resources/streamdeck/com.streamry.streamdeck.sdPlugin -ErrorAction SilentlyContinue
Copy-Item -Recurse streamdeck/com.streamry.streamdeck.sdPlugin src-tauri/resources/streamdeck/
```
