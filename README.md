<p align="center"><img src="docs/assets/icon.png" alt="StickerNest" width="112" height="112" /></p>
<h1 align="center">拾趣 · StickerNest</h1>
<p align="center">简体中文 · <a href="README_EN.md">English</a></p>
<p align="center">
  <a href="https://github.com/b1mango/StickerNest/releases/latest"><img src="https://img.shields.io/github/v/release/b1mango/StickerNest" alt="Release" /></a>
  <img src="https://img.shields.io/badge/platform-macOS_Apple_Silicon-325b43" alt="macOS Apple Silicon" />
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-f5c542" alt="MIT" /></a>
</p>

本地优先的 macOS 表情包管理器，收集、浏览和整理你的表情收藏。

## 功能

- 导入本地表情，提供微信、抖音导入流程。
- 大图标、小图标、列表、画廊四种视图，支持名称与标签搜索。
- 创建合集，单个或批量重命名、设置标签、整理归类。
- 扫描相似表情并选择保留项，误删内容可从回收站恢复。
- 导出选中原图，备份与恢复整个资料库。

## 安装使用

下载 [Apple Silicon 安装包](https://github.com/b1mango/StickerNest/releases/download/v0.0.1/StickerNest_0.0.1_aarch64.dmg)，打开 DMG，将 StickerNest 拖入「应用程序」。

启动后打开已有的 `StickerNest Library` 文件夹，或从备份恢复。首次建库目前需在源码目录执行以下命令，将输入路径替换为你的表情文件夹：

```sh
cargo run --manifest-path src-tauri/Cargo.toml --bin import-local -- \
  --input /path/to/stickers --library ~/StickerNest --create --source 本地
```

## 开发

需要 Node.js、Rust 和 macOS Command Line Tools。

```sh
npm ci
npm run desktop
npm run desktop:build
```

React + TypeScript 构建界面，Tauri + Rust 处理本地文件、相似识别与备份；`src/` 为界面代码，`src-tauri/src/` 为原生逻辑。

## 许可证

[MIT](LICENSE) © b1mango
