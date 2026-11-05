# Application assets

## Current icon set

The [18-app icon set](branding/app-suite/README.md) is design edition 7, approved
on 2026-11-05 using the built-in imagegen tool. It includes 18 raster masters,
72 opaque RGB PNG exports (1024 / 512 / 256 / 128 px), and two preview sheets.

Edition 7 applies the approved close-up compositions to all exports, preserving
each creature's identifying features. Motrune now uses Bifang and Sonune uses
four-legged Diting, generated using imagegen. Diting was refined using its own
draft and this suite's Yinglong icon as inputs; these inputs and prompts are
retained in [generation/edition-07](branding/app-suite/generation/edition-07/).
The other 16 masters remain byte-for-byte unchanged, including the four Office
masters from edition 3. Office symbols retain their smaller, complete layout.
No upstream, ancient-art or modern character images were supplied as inputs.
The same ivory engraving language is used across the suite.

[Mythology notes](branding/app-suite/mythology-notes.md) record the subjects,
pronunciations, application associations and textual reference sources.
Some silhouettes, including the Kunpeng transformation and Baize, are original
artistic interpretations rather than historical reconstructions. This describes
the design process, not legal clearance.

Exact prompts, subjects, colors, approved framing and master SHA-256 values are recorded in
[redesign-spec.json](branding/app-suite/redesign-spec.json),
[generation-prompts.txt](branding/app-suite/generation-prompts.txt) and
[names.json](branding/app-suite/names.json). Masters are raster PNGs, not editable
vectors or separated foreground/background layers. Exported icons retain full
square backgrounds; the preview gallery applies optional masks only for display.

## Runtime resources and exports

`python3 tools/export_icons.py` uses Pillow to crop and resize the full masters
using each design's approved `framing`, bake the composition into each PNG, and
synchronize all 18 product icons and entry icons, plus 17 Rust in-app marks:

- `AppScope/resources/base/media/<product>_icon.png` — 512 px.
- `entries/<product>/src/main/resources/base/media/app_icon.png` — 512 px.
- `branding/<product>-128.png` — 128 px for editors with a native crate.

Names and bundle identities come from [config/apps.json](config/apps.json).
The supported devices remain `tablet` and `2in1`. Native applications need a
rebuild to consume changed embedded images.

The exporter also creates a standalone offline gallery at
`dist/branding/zaoyu-app-icons.html` and a ZIP of the gallery, icons, application
identity data and design records at `dist/branding/zaoyu-app-icons.zip`.

## Upstream attribution

No upstream ArtCraft logo is used as a HarmonyOS application icon or in-app mark.
Upstream application names are retained only for source attribution. The current
icons do not imply upstream endorsement. Upstream asset inventories remain in
`ASSETS.md` or `ATTRIBUTION.md` at the Git revisions pinned in Cargo.toml.
Software, icon and font license texts remain in `licenses/<upstream>/`.
