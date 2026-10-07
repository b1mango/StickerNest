<div align="center">

<img src="docs/assets/icon.png" width="128" alt="StickerNest icon" />

# StickerNest · 拾趣

A local-first sticker library for your Mac: collect, organize, dedupe and export — all on your own machine.

[简体中文](README.md) · **English**

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8D8.svg)](https://tauri.app)
[![React](https://img.shields.io/badge/React-19-61DAFB.svg)](https://react.dev)
[![Platform](https://img.shields.io/badge/Platform-macOS-lightgrey.svg)](https://www.apple.com/macos)

</div>

## Features

- **Four views**: large icons / small icons / list / gallery, with a sticky page header while scrolling
- **Finder-style selection**: click to select, Shift-click ranges, rubber-band marquee, ⌘A, Esc
- **Collections**: create empty collections any time; copy / move stickers via the right-click menu
- **Tags & search**: batch rename, batch tags, full-text search over names and tags
- **Similar-items review**: scan for exact, similar and animation duplicates; keep one or ignore per group
- **Version groups & trash**: collapse multi-version stickers; deleted items stay recoverable
- **Export & backup**: export selected stickers byte-for-byte; full-library backups with SHA-256 verification

## Getting started

Requires Node.js, Rust and macOS Command Line Tools.

```sh
npm ci
npm run desktop   # launch the desktop app in dev mode
```

Create a library once (pick any parent folder):

```sh
mkdir -p ~/StickerNest && cargo run --offline --manifest-path src-tauri/Cargo.toml \
  --bin import-local -- --input /tmp --library ~/StickerNest --create --source 本地
```

Then open the generated `StickerNest Library` folder via “打开资料库” in the app.

Native build:

```sh
npm run tauri -- build --debug --bundles app
# output: src-tauri/target/debug/bundle/macos/StickerNest.app
```

## How it's built

| Layer | Tech |
| --- | --- |
| UI | React 19 + TypeScript + Vite, Finder-like interactions |
| Shell | Tauri 2 (WKWebView + native dialogs) |
| Core | Rust: plain-file library (`assets/` + `library.json` + `management.json`), perceptual-hash similarity, backup/restore, WeChat & Douyin import |

No accounts, no cloud, no telemetry; one library can only be opened by one process at a time.

## License

[MIT](LICENSE) © b1mango
