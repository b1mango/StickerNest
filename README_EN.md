# StickerNest

<p align="center">
  <img src="docs/assets/icon.png" alt="StickerNest" width="112" height="112" />
</p>

<p align="center">A local-first macOS sticker library for collecting, organizing, deduplicating, and exporting on your own machine.</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-unreleased-e3aa43" alt="status unreleased" />
  <img src="https://img.shields.io/badge/platform-macOS-1f6feb" alt="macOS" />
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-f5c542" alt="MIT" /></a>
</p>

<p align="center"><a href="README.md">简体中文</a> · English</p>

## Features

- Four views: large icons, small icons, list, and gallery.
- Click, Shift-click ranges, drag-to-select, ⌘A to select all, and Esc to cancel.
- Create collections and copy or move stickers from the context menu.
- Batch rename, batch tagging, and search across names and tags.
- Review exact, likely-similar, and animation duplicates in groups.
- Version groups and a recoverable trash.
- Byte-for-byte export in the original format and full-library backups with SHA-256 verification.

## Installation

No GitHub Release is published yet; run from source. You need Node.js, Rust, and the macOS Command Line Tools.

```sh
npm ci
npm run desktop
```

Create a library once (choose any parent directory):

```sh
mkdir -p ~/StickerNest && cargo run --offline --manifest-path src-tauri/Cargo.toml \
  --bin import-local -- --input /tmp --library ~/StickerNest --create --source 本地
```

Then choose “打开资料库” in the app and open the generated `StickerNest Library` folder.

## Development

```sh
npm run build
npm run test:core
npm run desktop:build -- --debug --bundles app
```

There are no accounts, cloud library, or telemetry. Libraries are stored as local files, and one library can only be opened by one process at a time.

## License

[MIT](LICENSE) © b1mango
