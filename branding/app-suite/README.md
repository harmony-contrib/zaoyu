# 造隅全系列名称与图标资源

2026-11-05 图标设计第 4 版，覆盖 18 款应用。中文品牌为 **造隅**，英文为 **ZAOYU**。
名称、包名与集成状态以 [config/apps.json](../../config/apps.json) 为准；设备为平板与 2in1。

## 当前设计

统一采用暖白主体、深色印刻线条与分色背景。
14 款创作应用采用中国神话异兽：凤凰、应龙、驳、金乌、腾蛇、麒麟、旋龟、狰、辟邪、夔、赑屃、獬豸、鲲鹏、白泽。
4 款 Office 应用保留第 3 版功能图形：文档与钢笔、单元格与图表、幻灯片、PDF 页面与书签。

14 张新母版由内置 imagegen 根据文字描述逐款生成，未输入上游图标、古画或现代影视游戏角色图片。
Office 四张母版逐字节保留。创作类的姿态、结构和纹样重新设计；鲲鹏的鱼鸟组合及白泽造型属于本次艺术化演绎。
题材与应用的联系是本次设计联想，详见 [异兽题材与参考说明](mythology-notes.md)。
这记录了设计过程，不表示完成了版权、商标或应用商店审查。

## 浏览与下载

- [离线交互图册](index.html)：依赖本目录的 PNG，支持搜索、分类、尺寸与遮罩预览、复制名称和包名、下载图标。
- 单文件版：`dist/branding/zaoyu-app-icons.html`，内嵌全部 72 张导出图，无需网络或相邻资源目录。
- 整套资源：`dist/branding/zaoyu-app-icons.zip`，含单文件 HTML、72 张 PNG、应用资料 CSV / JSON、预览图、设计提示词及题材参考说明。
- [全系列总览](overview.png) / [32–128px 尺寸与遮罩预览](size-and-mask-preview.png)。
- [设计清单 JSON](names.json) / [CSV](names.csv)：图案、读音、设计联想、用途、配色、文件路径与母版 SHA-256。
- [名称和包名 JSON](bundle-names.json) / [CSV](bundle-names.csv) / [TXT](bundle-names.txt)。
- [当前设计规格](redesign-spec.json) / [实际生成提示词](generation-prompts.txt)。

## 应用对应

| 图标 | 应用 | 中文名称 | 图案 / 读音 | 包名 |
| --- | --- | --- | --- | --- |
| <img src="icons/128/01-photo.png" width="48" height="48" alt="Pixrune"> | Pixrune | 造隅修图 | 凤凰 fèng huáng | `com.richerfu.pixrune` |
| <img src="icons/128/02-vector.png" width="48" height="48" alt="Vecrune"> | Vecrune | 造隅矢绘 | 应龙 yìng lóng | `com.richerfu.vecrune` |
| <img src="icons/128/03-cut.png" width="48" height="48" alt="Cliprune"> | Cliprune | 造隅剪辑 | 驳 bó | `com.richerfu.cliprune` |
| <img src="icons/128/04-light.png" width="48" height="48" alt="Lutrune"> | Lutrune | 造隅光影 | 金乌 jīn wū | `com.richerfu.lutrune` |
| <img src="icons/128/05-pdf.png" width="48" height="48" alt="Pagrune"> | Pagrune | 造隅阅编 | PDF 页面与书签  | `com.richerfu.pagrune` |
| <img src="icons/128/06-motion.png" width="48" height="48" alt="Motrune"> | Motrune | 造隅动效 | 腾蛇 téng shé | `com.richerfu.motrune` |
| <img src="icons/128/07-layout.png" width="48" height="48" alt="Layrune"> | Layrune | 造隅版式 | 麒麟 qí lín | `com.richerfu.layrune` |
| <img src="icons/128/08-interface.png" width="48" height="48" alt="Framune"> | Framune | 造隅界面 | 旋龟 xuán guī | `com.richerfu.framune` |
| <img src="icons/128/09-texture.png" width="48" height="48" alt="Texune"> | Texune | 造隅纹绘 | 狰 zhēng | `com.richerfu.texune` |
| <img src="icons/128/10-material.png" width="48" height="48" alt="Shadrune"> | Shadrune | 造隅材质 | 辟邪 bì xié | `com.richerfu.shadrune` |
| <img src="icons/128/11-write.png" width="48" height="48" alt="Quilrune"> | Quilrune | 造隅文档 | 文档与钢笔  | `com.richerfu.quilrune` |
| <img src="icons/128/12-audio.png" width="48" height="48" alt="Sonune"> | Sonune | 造隅音轨 | 夔 kuí | `com.richerfu.sonune` |
| <img src="icons/128/13-slides.png" width="48" height="48" alt="Deckune"> | Deckune | 造隅演示 | 幻灯片与图表  | `com.richerfu.deckune` |
| <img src="icons/128/14-cad.png" width="48" height="48" alt="Dimrune"> | Dimrune | 造隅工图 | 赑屃 bì xì | `com.richerfu.dimrune` |
| <img src="icons/128/15-sheets.png" width="48" height="48" alt="Gridrune"> | Gridrune | 造隅表格 | 单元格与数据图表  | `com.richerfu.gridrune` |
| <img src="icons/128/16-maker.png" width="48" height="48" alt="Modune"> | Modune | 造隅构建 | 獬豸 xiè zhì | `com.richerfu.modune` |
| <img src="icons/128/17-studio.png" width="48" height="48" alt="Visrune"> | Visrune | 造隅灵绘 | 鲲鹏 kūn péng | `com.richerfu.visrune` |
| <img src="icons/128/18-hub.png" width="48" height="48" alt="Nodune"> | Nodune | 造隅工坊 | 白泽 bái zé | `com.richerfu.nodune` |

## 导出与工程同步

```sh
python3 tools/export_icons.py
```

需安装 Pillow。脚本只负责等比缩放、复制与图册排版，不重新生成插画。
导出前校验母版 SHA-256 与规格清单一致，生成 1024、512、256、128px 不透明 RGB PNG。
母版尺寸记录于 `names.json`；所有交付图标均保留完整方形背景，圆角和圆形只用于预览。

512px 图标同步至 `AppScope/resources/base/media/<product>_icon.png` 和
`entries/<product>/src/main/resources/base/media/app_icon.png`；
17 款 Rust 编辑器的 128px 标志同步至 `branding/<product>-128.png`。
修改已嵌入 Rust 的图标后，需重新构建对应应用才会出现在安装包内。

图册模板为 [tools/icon-gallery.html](../../tools/icon-gallery.html)。
应用简介为现有用途字段生成的简短草稿，未代替完整上架资料。
上游软件与素材许可仍见 [ASSETS.md](../../ASSETS.md) 和 [licenses](../../licenses/)。
