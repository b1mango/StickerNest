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

- 从微信、抖音或本地文件夹导入，按来源归档；拖入图片或文件夹即可导入，支持 PNG、JPEG、GIF、WebP 与 HEIC。
- 抖音收藏一键采集：应用自带专用浏览器，首次扫码登录后自动读取收藏、下载原图并导入。
- 选中后 ⌘C 复制或直接拖到微信聊天窗口，WebP 会自动转成微信可用的 GIF / PNG。
- 网格与列表两种视图，名称搜索、收藏、最近使用，空格快速查看。
- 用合集整理表情，支持批量重命名；右键或拖到侧栏合集即可归类。
- 形象识别（可选）：在本机识别草地牛、奶蛙、chiikawa、熊猫头、龙图等形象，确认后归入同名合集。
- 扫描相似表情并选择保留项，误删内容可从回收站恢复。
- 导出选中原图，备份与恢复整个资料库；外观可跟随系统或固定浅色 / 深色。

## 安装使用

下载 [Apple Silicon 安装包](https://github.com/b1mango/StickerNest/releases/download/v0.1.0/StickerNest_0.1.0_aarch64.dmg)，打开 DMG，将 StickerNest 拖入「应用程序」。

首次启动选择「新建资料库」（选空文件夹即直接作为资料库），或打开已有资料库、从备份恢复；之后会自动打开上次使用的资料库。形象识别默认关闭，在侧栏「形象识别」中启用时下载约 176 MB 模型。

## 开发

需要 Node.js 24.22.1（见 `.nvmrc`）、Rust 和 macOS Command Line Tools；用其他 Node 版本运行 `npm run desktop:build` 时，构建脚本会自动下载并校验 24.22.1。桌面采集运行时还需 uv 已准备的可搬移 CPython 3.11 与 Pillow 13.0.0（`uv run --python 3.11 --with pillow==13.0.0 python -c "import PIL"`），或通过 `STICKERNEST_PYTHON` 指定同等环境。构建从本地准备并内置运行时，安装后的应用无需额外安装 Node.js/Python。

```sh
npm ci
npm run desktop
npm run desktop:build
```

React + TypeScript 构建界面，Tauri + Rust 处理本地文件、相似识别、形象识别与备份；`src/` 为界面代码，`src-tauri/src/` 为原生逻辑。

## 许可证

[MIT](LICENSE) © b1mango
