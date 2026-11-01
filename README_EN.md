<p align="center"><img src="docs/assets/icon.png" alt="StickerNest" width="112" height="112" /></p>
<h1 align="center">拾趣 · StickerNest</h1>
<p align="center"><a href="README.md">简体中文</a> · English</p>
<p align="center">
  <a href="https://github.com/b1mango/StickerNest/releases/latest"><img src="https://img.shields.io/github/v/release/b1mango/StickerNest" alt="Release" /></a>
  <img src="https://img.shields.io/badge/platform-macOS_Apple_Silicon-325b43" alt="macOS Apple Silicon" />
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-f5c542" alt="MIT" /></a>
</p>

A local-first macOS app for collecting, browsing, and organizing stickers.

## Features

- Import from WeChat, Douyin, or local folders, archived by source; drop images or folders onto the window to import.
- Press ⌘C or drag a sticker straight into a WeChat chat; Douyin WebP files are converted to WeChat-friendly GIF / PNG automatically.
- Grid and list views, name search, favorites, recently used, and Space for Quick Look.
- Organize with collections and batch rename; right-click or drag onto a sidebar collection to file stickers.
- Review similar stickers and choose which to keep; restore items from Trash.
- Export selected originals, back up and restore libraries; follows the system light / dark appearance.

## Installation

Download the [Apple Silicon DMG](https://github.com/b1mango/StickerNest/releases/download/v0.0.2/StickerNest_0.0.2_aarch64.dmg), open it, and drag StickerNest into Applications.

On first launch, create a new library, open an existing `StickerNest Library` folder, or restore a backup. The last library reopens automatically.

## Development

Requires Node.js 24.22.1, Rust, and macOS Command Line Tools. Prepare relocatable CPython 3.11 and Pillow 13.0.0 with uv (`uv run --python 3.11 --with pillow==13.0.0 python -c "import PIL"`), or set `STICKERNEST_PYTHON` to an equivalent environment. The build bundles local runtimes; installed apps do not require Node.js or Python.

```sh
npm ci
npm run desktop
npm run desktop:build
```

React + TypeScript provides the interface; Tauri + Rust handles local files, similarity scanning, and backups. UI code lives in `src/`, native logic in `src-tauri/src/`.

## License

[MIT](LICENSE) © b1mango
