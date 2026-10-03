# 拾趣 · StickerNest

个人本地表情资料库，Mac 桌面应用。支持普通图片、GIF和动画WebP导入、精确去重、名称/标签/合集整理、筛选与预览。抖音网页收藏已有本地采集辅助工具；微信原件读取与平台收藏批量写入尚未实现。

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
3. 点击素材，在详情中编辑名称、标签与合集；多个标签或合集用逗号分隔。可搜索名称或标签，按来源、标签和合集筛选。
4. 展开“来源与精确去重”，新增本地账号别名或选择已有账号，再导入抖音采集工具的 `report.json`。此处只读取报告，不下载文件；请先将报告引用的资源入库。同一账号再次导入时选择已有账号。
5. 关闭应用后，可整体复制资料库进行手动备份；搬移后重新选择该文件夹。

本地文件为 `assets/` 原始素材、`library.json` 素材索引及 `management.json` 整理记录。名称、标签、合集、账号与收藏引用仅写入整理记录，不改原件及素材索引。备份应复制整个资料库；请勿单独改名或移动其中素材。同一资料库一次只允许一个进程打开。不使用云数据库或同步服务。

去重统计区分已记录收藏、引用素材与共用文件组，仅覆盖已导入来源报告。每个账号最近记录批次另列清单、已映射、失败和未处理数；部分报告会提示“来源映射待补”，不能据此判断平台完整收藏。资源版本保留历史，重复导入相同报告不会增加收藏计数。损坏的整理文件会阻止编辑，保留原文件并提示检查后重开。

支持静态 PNG/JPEG/WebP、GIF及动画WebP；APNG/HEIC暂不支持。单文件最多20MiB、宽高4096像素；动画最多1000帧、累计1亿解码像素。这些是应用资源限制，不是平台限制。

## 验证与进度

```sh
npm run test:core
npm run build
```

见[项目设计](项目设计.md)、[项目进度](项目进度.md)、[对话记录](对话记录.md)。原生小样本创建、导入、预览、GIF及自建动画WebP播放、切库重开已通过；抖音590项资源已取得，精确去重587素材，原生已打开显示587项。实际平台动画播放、管理模块原生交互及完整进程重启仍待验收；管理持久化核心测试与浏览器模拟IPC检查已通过。实际1500素材规模尚未验收。

相似图识别、批量编辑、回收站、自动备份及通用导出尚未实现。微信仅完成公开资料调研和安卓小号缓存实验，尚未取得可解码原件。

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
