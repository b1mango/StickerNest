# 拾趣 · StickerNest

个人本地表情资料库，Mac 桌面应用。当前模块支持普通图片/GIF导入、精确去重、来源筛选、搜索与预览。微信/抖音手机自动导出尚未实现。

## 启动

需要 Node.js、Rust 与 macOS Command Line Tools。

```sh
npm ci
npm run desktop
```

构建本机测试版：

```sh
npm run tauri -- build --debug --bundles app
```

产物：`src-tauri/target/debug/bundle/macos/StickerNest.app`。

`npm run dev` 仅用于浏览器界面预览，本地资料库功能必须在桌面应用使用。

## 使用

1. 新建资料库时选择父文件夹，应用创建 `StickerNest Library` 子文件夹；已有资料库使用“打开”。
2. 选择来源并导入文件。来源用于整理，不会连接微信或抖音。
3. 搜索名称、按来源筛选，点击素材查看原图与信息。
4. 关闭应用后，可整体复制资料库进行手动备份；搬移后重新选择该文件夹。

本地文件为 `assets/` 原始素材与 `library.json` 管理信息。请勿单独改名或移动其中素材，否则预览或重开可能失败。同一资料库一次只允许一个进程打开。

支持静态 PNG/JPEG/WebP 与 GIF；APNG/动画 WebP 暂不支持。单文件最多20MiB、宽高4096像素；GIF最多1000帧、累计1亿解码像素。这些是应用资源限制，不是平台限制。

## 验证与进度

```sh
npm run test:core
npm run build
```

见[项目设计](项目设计.md)、[项目进度](项目进度.md)、[对话记录](对话记录.md)。原生小样本创建、导入、预览、GIF播放及切库重开已通过；完整进程重启和实际1500素材规模尚待验收。安卓ADB已准备，等待设备连接。
