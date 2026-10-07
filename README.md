# StickerNest

<p align="center">
  <img src="docs/assets/icon.png" alt="StickerNest" width="112" height="112" />
</p>

<p align="center">本地优先的 macOS 表情包管理器，收集、整理、查重与导出都在你的电脑上完成。</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-unreleased-e3aa43" alt="status unreleased" />
  <img src="https://img.shields.io/badge/platform-macOS-1f6feb" alt="macOS" />
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-f5c542" alt="MIT" /></a>
</p>

<p align="center">简体中文 · <a href="README_EN.md">English</a></p>

## 功能

- 大图标、小图标、列表和画廊四种浏览方式。
- 点击、Shift 连选、拖拽框选、⌘A 全选与 Esc 取消选择。
- 新建合集，并通过右键菜单复制或移动表情。
- 批量重命名、批量设置标签，按名称和标签搜索。
- 识别完全相同、疑似相似和相同动画，逐组保留或忽略。
- 版本分组与可恢复回收站。
- 原格式原字节导出，整库备份带 SHA-256 校验。

## 安装

当前尚未发布 GitHub Release，请从源码运行。需要 Node.js、Rust 和 macOS Command Line Tools。

```sh
npm ci
npm run desktop
```

首次使用先创建一个资料库（任选父目录）：

```sh
mkdir -p ~/StickerNest && cargo run --offline --manifest-path src-tauri/Cargo.toml \
  --bin import-local -- --input /tmp --library ~/StickerNest --create --source 本地
```

之后在应用中选择“打开资料库”，打开生成的 `StickerNest Library` 文件夹。

## 开发

```sh
npm run build
npm run test:core
npm run desktop:build -- --debug --bundles app
```

应用没有账号、云库或遥测；资料库使用本地文件保存，同一资料库同一时间只允许一个进程打开。

## License

[MIT](LICENSE) © b1mango
