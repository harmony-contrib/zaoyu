# Application assets

| Asset | Source | Purpose / rights |
| --- | --- | --- |
| `branding/vecrune-128.png` | User-requested original Zaoyu icon set, generated 2026-11-04; `02-vector.png` | Vecrune in-app mark; independent replacement for upstream mark |
| `AppScope/resources/base/media/vecrune_icon.png` | Same original Zaoyu vector icon | Vecrune app resource |
| `entries/vecrune/src/main/resources/base/media/app_icon.png` | Same original Zaoyu vector icon | Launcher / launch screen |
| `AppScope/resources/base/media/nodune_icon.png` | Original Zaoyu icon set, `18-hub.png`, generated 2026-11-04 | Nodune app resource |
| `entries/nodune/src/main/resources/base/media/app_icon.png` | Same original Zaoyu hub icon | Launcher / launch screen |

The following applications reuse the same user-requested original icon set,
generated 2026-11-04. Each has a 128 px in-app mark at
`branding/<product>-128.png`, a launcher resource at
`entries/<product>/src/main/resources/base/media/app_icon.png`, and a product
resource at `AppScope/resources/base/media/<product>_icon.png`.

| Product | Original icon |
| --- | --- |
| Pixrune | `01-photo.png` |
| Cliprune | `03-cut.png` |
| Pagrune | `05-pdf.png` |
| Motrune | `06-motion.png` |
| Layrune | `07-layout.png` |
| Gridrune | `15-sheets.png` |

No upstream ArtCraft logo is used as a HarmonyOS application icon or in-app mark.
The independent icons do not imply upstream endorsement. Upstream's own asset
inventory is in `ASSETS.md` or `ATTRIBUTION.md` at each Git revision pinned in Cargo.toml.
Software, icon and font license texts are retained in `licenses/<upstream>/`;
do not assume software licenses cover trademarks.
