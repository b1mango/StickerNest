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

- Import from WeChat, Douyin, or local folders, archived by source; drop images or folders onto the window to import. PNG, JPEG, GIF, WebP and HEIC are supported.
- One-click Douyin collection: a built-in dedicated browser reads your saved stickers, downloads the originals, and imports them after a one-time QR login.
- Press ⌘C or drag a sticker straight into a WeChat chat; WebP files are converted to WeChat-friendly GIF / PNG automatically.
- Grid and list views, name search, favorites, recently used, and Space for Quick Look.
- Organize with collections and batch rename; right-click or drag onto a sidebar collection to file stickers.
- Character recognition (optional): recognizes characters such as 草地牛, 奶蛙, chiikawa, 熊猫头 and 龙图 on device and files them into matching collections after you confirm.
- Review similar stickers and choose which to keep; restore items from Trash.
- Export selected originals, back up and restore libraries; follow the system appearance or pick light / dark.

## Installation

Download the [Apple Silicon DMG](https://github.com/b1mango/StickerNest/releases/download/v0.1.0/StickerNest_0.1.0_aarch64.dmg), open it, and drag StickerNest into Applications.

On first launch, create a new library (an empty folder becomes the library itself), open an existing one, or restore a backup. The last library reopens automatically. Character recognition is off by default; enabling it from the sidebar downloads a model of about 176 MB.

## Development

Requires Node.js 24.22.1 (see `.nvmrc`), Rust, and macOS Command Line Tools; when `npm run desktop:build` runs under another Node version, the build script downloads and verifies 24.22.1 automatically. Prepare relocatable CPython 3.11 and Pillow 13.0.0 with uv (`uv run --python 3.11 --with pillow==13.0.0 python -c "import PIL"`), or set `STICKERNEST_PYTHON` to an equivalent environment. The build bundles local runtimes; installed apps do not require Node.js or Python.

```sh
npm ci
npm run desktop
npm run desktop:build
```

React + TypeScript provides the interface; Tauri + Rust handles local files, similarity scanning, character recognition, and backups. UI code lives in `src/`, native logic in `src-tauri/src/`.

## License

[MIT](LICENSE) © b1mango
