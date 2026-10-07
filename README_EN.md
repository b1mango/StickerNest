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

- Import local stickers, with WeChat and Douyin import workflows.
- Browse large icons, small icons, lists, or a gallery; search names and tags.
- Organize collections, rename and tag stickers individually or in batches.
- Review similar stickers and choose which to keep; restore items from Trash.
- Export selected originals, back up libraries, and restore backups.

## Installation

Download the [Apple Silicon DMG](https://github.com/b1mango/StickerNest/releases/download/v0.0.1/StickerNest_0.0.1_aarch64.dmg), open it, and drag StickerNest into Applications.

Open an existing `StickerNest Library` folder or restore a backup. To create your first library from source, use the command below with your sticker folder:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --bin import-local -- \
  --input /path/to/stickers --library ~/StickerNest --create --source 本地
```

## Development

Requires Node.js, Rust, and macOS Command Line Tools.

```sh
npm ci
npm run desktop
npm run desktop:build
```

React + TypeScript provides the interface; Tauri + Rust handles local files, similarity scanning, and backups. UI code lives in `src/`, native logic in `src-tauri/src/`.

## License

[MIT](LICENSE) © b1mango
