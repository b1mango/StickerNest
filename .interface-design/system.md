# 拾趣 · StickerNest 设计系统笔记

## 方向与感觉

个人表情收藏画册：纸感表面（parchment）+ 墨色文字 + 森林绿单一 accent；内容（表情图）永远为主，chrome 尽量隐形。密集控制区（toolbar/caption）与松内容区（网格）的节奏对比是刻意的。

## Token 与深度

- 三级表面：paper → paper-raised → paper-inset（输入控件一律 inset，不调亮）。
- 深度策略：border 为主（rgba rule,findable but not demanding）；选中/聚焦用 `forest` 与 `rgba(50,91,67,.4)` 的 ring,不叠阴影。
- radius 分级：8 控件 / 12 卡片 / 20 对话框。
- 单 hue、单 accent；来源区分靠文字与图标，不新增彩色。

## 选中系统（关键组件）

- `.select-box`:22px 圆形、自绘、-11px 伪元素扩展到 44 命中；hover 才淡入，勾中则森林绿实心 + 白色 SVG 勾。
- 卡选中 = `.sticker-card.checked`:image 区 forest-wash 底 + forest 边，名称变 forest。hover 仅 `rule-strong` 边，绝不与选中混淆。
- “已选 N 项”用 forest 胶囊计数。

## 布局习惯

- 侧栏与画布同纸色，仅 1px rule 边；操作区分三组：资料库动作（gap 10)、次级文字动作（border-top 分组）、本地存储指示。
- grid-actions 同组 text-button 36px 高、padding 10、hover 从 secondary → ink；caption 行基线对齐。
- 展开内容（group-row）占 grid 整行（1/-1)，不挤卡宽。

## 硬检查

- hit ≥ 40（文字按钮例外需伪元素）;focus-visible 2px `rgba(50,91,67,.4)` + 2px offset;press 为 scale(.98),reduced-motion 禁用；tabular-nums 用于所有动态数字；图片 with 1px inset outline rgba(0,0,0,.06)。
