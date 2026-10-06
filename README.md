# 拾趣 · StickerNest

个人本地表情资料库，Mac 桌面应用（Tauri 2 + React）。把散落在各处的表情收进一个安静的画册：导入、整理、查重、导出都在本机完成，不用云、不上传。

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

产物：`src-tauri/target/debug/bundle/macos/StickerNest.app`。`npm run dev` 仅用于浏览器界面预览，资料库功能必须在桌面应用中使用。

## 界面一览

- **我的资料库**：全部表情 / 微信 / 抖音 / 本地文件，按来源查看。
- **合集**：自己创建的合集名单，支持空合集；计数实时更新。
- **整理**：相似项对比（扫描相似素材）、回收站。
- **顶栏**：搜索（⌘F）、标签筛选、显示模式切换、导出；右上角唯一的「导入表情」入口（本地文件 / 微信 / 抖音收藏从下拉选）。

## 选择与操作（仿访达）

- 点击表情 = 选中/取消选中；**Shift+点击** = 选中从上个锚点到当前之间的整段。
- 在网格空白处**按住拖拽** = 框选；按住 Shift 框选 = 在现有选择上追加。
- 点击空白 = 取消选择；⌘A = 全选当前列表。
- 右键表情 = 多功能菜单：查看详情、复制到 / 移动到合集、批量重命名、批量标签、加入版本分组、导出、移入回收站。
- 回收站里的点击 = 查看详情（可恢复）。

## 显示模式（仿访达）

大图标、小图标、列表（名称/格式/尺寸/大小/导入时间）、画廊（大图 + 缩略图条，方向键切换），选择会记忆。

## 合集

1. 侧栏「新建合集」先建一个空合集（已勾选表情时会一并放入）。
2. 之后任意位置勾选表情 → 右键「复制到合集」或「移动到合集」（移动 = 加入目标合集并移出当前合集）。
3. 合集本质是整理记录里的标签，不动原文件；一个表情可属于多个合集。

## 整理

- **版本分组**：多个版本归为一组，列表只显示主版本，卡片可展开查看全部版本、拆分组；组可设统一的标签与合集。
- **相似项对比**：扫描完全相同、疑似相似、疑似相同的动画，逐组选择「保留」哪张（其余进回收站，可恢复）或「忽略」（之后不再提示）。
- **回收站**：逻辑删除，文件仍在本地，随时恢复。

## 导出与备份

**导出所选 / 导出当前列表**：复制原格式原字节到新建导出文件夹，按显示名重命名；勾选「含全部版本」时连同所在分组的全部版本一起导出。

**备份这个库**：在你选的位置新建 `StickerNest Backup <时间戳>/` 文件夹（不是单文件、不是压缩包），内容为：

```
StickerNest Backup 20261007-1530/
├── assets/                 # 全部素材原件（哈希文件名）
├── library.json            # 素材索引
├── management.json         # 整理记录（名称/标签/合集/分组/回收站等）
└── backup-manifest.json    # 每个文件的大小与 SHA-256，complete 标记
```

写入时逐文件校验哈希后才标记完成。**从备份恢复**：选择备份文件夹 → 重新逐文件校验哈希 → 复制为新的 `StickerNest Library` 并打开，原备份保持不变。想手动备份时也可整体复制资料库文件夹。

## 资料库结构

```
StickerNest Library/
├── assets/           # 素材原件（内容与文件名由应用管理，勿手动改名或移动）
├── library.json      # 素材索引
└── management.json   # 整理记录（损坏时只停用编辑，不会被覆盖）
```

同一资料库一次只允许一个进程打开；不使用云数据库或同步服务。

## 支持格式与限制

静态 PNG/JPEG/WebP、GIF 及动画 WebP；APNG/HEIC 暂不支持。单文件最多 20MiB、宽高 4096 像素；动画最多 1000 帧、累计 1 亿解码像素。这些是应用资源限制，不是平台限制。

## 验证

```sh
npm run build       # 前端类型检查与构建
npm run test:core   # Rust 核心测试
```

## 抖音本地采集辅助工具

先通过本人已登录网页的收藏面板取得并核验清单；当前不会自动连接浏览器，也不提供官方收藏 API。输入为包含 `id_str`、`hash`、`animate_url/static_url`（`uri`、`url_list`）的本地 JSON 数组。签名地址有时效，清单仅保存在忽略的 `output/` 下。

需要 Python 3 和 Pillow。下载原件与逐项报告：

```sh
python3 scripts/download_douyin.py output/web-probe/verified-stickers.json output/douyin-export
```

资源保存在 `originals/`，结果为 `report.json`。相同命令可恢复：先检查资源身份和本地哈希，已保存的资源不重复下载。地址过期需重新获取清单；不自动登录或提取 Cookie。仅允许当前验证过的两个表情 CDN 域名，出现新域名需审查后更新。图片校验失败仍保留原件并标为待核验；下载超过 20MiB 直接拒绝。

命令行导入（父目录需先存在）：

```sh
mkdir -p output/douyin-library
cargo run --offline --manifest-path src-tauri/Cargo.toml --bin import-local -- \
  --input output/douyin-export/originals --library output/douyin-library \
  --create --source 抖音 > output/douyin-export/import-report.json
```

已有库省略 `--create`，`--library` 指向 `StickerNest Library` 本身。输入目录只放资源文件，不递归，拒绝链接、子目录及未知扩展名。不要在桌面应用和命令行同时打开同一库。

辅助工具测试：`python3 -m unittest discover -s tests -v` 与 `cargo test --offline --manifest-path src-tauri/Cargo.toml --bin import-local`。
