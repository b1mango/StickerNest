<div align="center">

<img src="docs/assets/icon.png" width="128" alt="拾趣 StickerNest 图标" />

# 拾趣 · StickerNest

本地优先的表情包管理器：收集、整理、查重、导出，全在你自己电脑上。

**简体中文** · [English](README_EN.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8D8.svg)](https://tauri.app)
[![React](https://img.shields.io/badge/React-19-61DAFB.svg)](https://react.dev)
[![Platform](https://img.shields.io/badge/Platform-macOS-lightgrey.svg)](https://www.apple.com/macos)

</div>

## 功能

- **四种浏览**：大图标 / 小图标 / 列表 / 画廊，页头工具栏滚动时固定
- **顺手的选择**：点击选中、Shift 连选、空白拖拽框选、⌘A 全选、Esc 取消
- **合集**：随时新建空合集，右键把表情「复制到 / 移动到合集」
- **标签与搜索**：批量重命名、批量设置标签，名称与标签全文检索
- **相似项对比**：扫描完全相同、疑似相似与相同动画，逐组保留或忽略
- **版本分组与回收站**：多版本折叠展示；删除先进回收站，随时恢复
- **导出与备份**：导出所选为原格式原字节；整库备份带 SHA-256 校验

## 快速开始

需要 Node.js、Rust 与 macOS Command Line Tools。

```sh
npm ci
npm run desktop   # 开发模式启动桌面应用
```

首次使用先创建一个资料库（任选个父目录）：

```sh
mkdir -p ~/StickerNest && cargo run --offline --manifest-path src-tauri/Cargo.toml \
  --bin import-local -- --input /tmp --library ~/StickerNest --create --source 本地
```

之后在应用里「打开资料库」，选取生成好的 `StickerNest Library` 文件夹即可。

构建本机安装包：

```sh
npm run tauri -- build --debug --bundles app
# 产物：src-tauri/target/debug/bundle/macos/StickerNest.app
```

## 技术路径

| 层 | 实现 |
| --- | --- |
| 界面 | React 19 + TypeScript + Vite，仿访达交互 |
| 壳 | Tauri 2（WKWebView + 原生文件对话框） |
| 核心 | Rust：资料库（`assets/` + `library.json` + `management.json` 纯文件）、感知哈希相似识别、备份/恢复、微信与抖音表情导入 |

无账号、无云库、无遥测；同一资料库同一时间仅允许一个进程打开。

## 规划（待实现）

- AI 批量重命名：按画面内容自动起名
- AI 识别分类：自动打标签、归到合集

## 许可

[MIT](LICENSE) © b1mango
