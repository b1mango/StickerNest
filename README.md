# 拾趣 · StickerNest

个人本地表情资料库，Mac 桌面应用。当前模块支持普通图片、GIF和动画WebP导入、精确去重、来源筛选、搜索与预览。微信/抖音手机自动导出尚未实现。

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

支持静态 PNG/JPEG/WebP、GIF及动画WebP；APNG/HEIC暂不支持。单文件最多20MiB、宽高4096像素；动画最多1000帧、累计1亿解码像素。这些是应用资源限制，不是平台限制。

## 验证与进度

```sh
npm run test:core
npm run build
```

见[项目设计](项目设计.md)、[项目进度](项目进度.md)、[对话记录](对话记录.md)。原生小样本创建、导入、预览、GIF播放及切库重开已通过；完整进程重启和实际1500素材规模尚待验收。抖音网页590项收藏清单与静态/动画小样本已验证；微信候选缓存尚未解码，自动采集功能未实现。

## 抖音本地采集辅助工具

先通过本人已登录网页的收藏面板取得并核验清单；当前不会自动连接浏览器，也不提供官方收藏API。输入为包含 `id_str`、`hash`、`animate_url/static_url`（`uri`、`url_list`）的本地JSON数组。签名地址有时效，清单仅保存在忽略的 `output/` 下。

需要 Python 3 和 Pillow。下载原件与逐项报告：

```sh
python3 scripts/download_douyin.py output/web-probe/verified-stickers.json output/douyin-export
```

资源保存在 `originals/`，结果为 `report.json`。相同命令可恢复：先检查资源身份和本地哈希，已保存的资源不重复下载。地址过期需重新获取清单；不自动登录或提取Cookie。仅允许当前验证过的两个表情CDN域名，出现新域名需审查后更新。图片校验失败仍保留原件并标为待核验；下载超过20MiB直接拒绝。报告的收藏条目数、成功资源数、内容去重数分别计算。

新建独立资料库并导入（父目录需先存在）：

```sh
mkdir -p output/douyin-library
cargo run --offline --manifest-path src-tauri/Cargo.toml --bin import-local -- \
  --input output/douyin-export/originals --library output/douyin-library \
  --create --source 抖音 > output/douyin-export/import-report.json
```

已有库省略 `--create`，`--library` 指向 `StickerNest Library` 本身。输入目录只放资源文件，不递归，拒绝链接、子目录及未知扩展名；原始内容仍由Rust校验。部分素材失败不会回滚已成功入库项，退出码2并列出失败文件；其他错误退出码1。可在桌面应用“打开资料库”中选取新库。不要在桌面和命令行同时打开同一库。

辅助工具测试：`python3 -m unittest discover -s tests -v` 与 `cargo test --offline --manifest-path src-tauri/Cargo.toml --bin import-local`。
