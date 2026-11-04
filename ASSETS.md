# Application assets

## Complete suite design archive

The [18-app design archive](branding/app-suite/README.md) contains the original
user-requested Zaoyu icon set generated on 2026-11-04: 18 PNG masters at 1254 px,
18 exports at each of 1024 / 512 / 256 / 128 px, and two original preview sheets.
It also includes single-word English and Chinese names, `com.richerfu.*` bundle
name mappings, palette and motif descriptions, original generation prompts,
naming notes, and an offline HTML gallery.

PNG files and original generation/naming records are preserved unchanged.
The JSON, CSV, text mappings and gallery use the bundle names from
[`config/apps.json`](config/apps.json). The project supports `tablet` and `2in1`;
phone references in the original preview sheets and prompts are historical.
The archive includes planned applications and does not add build products.
It is a design resource archive, not a source of upstream application code.

The master image SHA-256 values and stable icon filenames are recorded in
[`names.json`](branding/app-suite/names.json). All icon masters are raster PNGs;
editable vector or separated foreground/background assets are not included.

## Runtime assets

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
| Lutrune | `04-light.png` |
| Pagrune | `05-pdf.png` |
| Motrune | `06-motion.png` |
| Layrune | `07-layout.png` |
| Framune | `08-interface.png` |
| Texune | `09-texture.png` |
| Shadrune | `10-material.png` |
| Quilrune | `11-write.png` |
| Sonune | `12-audio.png` |
| Deckune | `13-slides.png` |
| Dimrune | `14-cad.png` |
| Gridrune | `15-sheets.png` |
| Modune | `16-maker.png` |
| Visrune | `17-studio.png` |

No upstream ArtCraft logo is used as a HarmonyOS application icon or in-app mark.
The independent icons do not imply upstream endorsement. Upstream's own asset
inventory is in `ASSETS.md` or `ATTRIBUTION.md` at each Git revision pinned in Cargo.toml.
Software, icon and font license texts are retained in `licenses/<upstream>/`;
do not assume software licenses cover trademarks.
