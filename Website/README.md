# Streamry website

Static marketing site for [Streamry](../README.md), deployed with **GitHub Pages**.

## Pages

- `index.html` — product home / features
- `downloads.html` — Windows & macOS installers
- `downloads/` — published EXE and DMG files
- `version.json` — latest app version (used by the desktop app for update checks)

When you ship a new build, bump `version` in `version.json` (and the installers under `downloads/`) to match `package.json` / `src-tauri/tauri.conf.json`.

## Local preview

```bash
npx --yes serve Website
```

## Publish on GitHub Pages

The workflow at `.github/workflows/pages.yml` deploys this folder on pushes to `main`.

1. Push the repo to GitHub.
2. **Settings → Pages → Build and deployment → Source:** GitHub Actions.
3. After the workflow runs: `https://techjeeper.github.io/Streamry/`.

## Screenshots

With the Vite UI running (`npm run tauri dev` or `npm run dev`):

```bash
node Website/scripts/capture-screens.mjs
```

Writes PNGs to `Website/assets/screens/`.
