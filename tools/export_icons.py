#!/usr/bin/env python3
"""Export the approved icon masters, sync runtime assets, and package the offline atlas.

Requires Pillow. Artwork is created with imagegen; this script applies approved
framing, resizes and packages the masters. Run: python3 tools/export_icons.py
"""

import base64
import csv
import hashlib
import json
import math
from pathlib import Path
import shutil
import zipfile

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
SUITE = ROOT / "branding/app-suite"
OUT = ROOT / "dist/branding"
SIZES = (1024, 512, 256, 128)


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def write_csv(path, rows):
    with path.open("w", encoding="utf-8-sig", newline="") as handle:
        csv.writer(handle, lineterminator="\n").writerows(rows)


def data_url(path):
    return "data:image/png;base64," + base64.b64encode(path.read_bytes()).decode("ascii")


def framed_icon(source, size, design):
    """Bake the approved CSS translate(x%, y%) scale(s) into a square PNG.

    Translation is relative to the unscaled icon frame, with a centered origin.
    Keep fractional crop coordinates and sample directly from the full master.
    A translated source can expose a thin strip of the preview's background.
    """
    framing = design["framing"]
    scale, x, y = (float(framing[key]) for key in ("scale", "x", "y"))
    if not all(math.isfinite(value) for value in (scale, x, y)) or scale <= 0:
        raise ValueError(f"Invalid icon framing: {framing}")
    width, height = source.size
    left = (0.5 - (0.5 + x / 100) / scale) * width
    top = (0.5 - (0.5 + y / 100) / scale) * height
    right, bottom = left + width / scale, top + height / scale
    padding = (max(0, math.ceil(-left)), max(0, math.ceil(-top)),
               max(0, math.ceil(right - width)), max(0, math.ceil(bottom - height)))
    if any(padding):
        pad_left, pad_top, pad_right, pad_bottom = padding
        canvas = Image.new("RGB", (width + pad_left + pad_right, height + pad_top + pad_bottom), design["bg"])
        canvas.paste(source, (pad_left, pad_top))
        source = canvas
        left, right = left + pad_left, right + pad_left
        top, bottom = top + pad_top, bottom + pad_top
    return source.resize((size, size), Image.Resampling.LANCZOS, box=(left, top, right, bottom))


def previews(apps, edition):
    """Contact sheets for checking approved art, without altering icon pixels."""
    candidates = [Path("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"),
                  Path("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")]
    face = next((p for p in candidates if p.is_file()), None)

    def font(size):
        return ImageFont.truetype(str(face), size) if face else ImageFont.load_default(size=size)

    sheet = Image.new("RGB", (1800, 1280), "#F4F1EA")
    draw = ImageDraw.Draw(sheet)
    draw.text((66, 38), "ZAOYU / 造隅", fill="#17231E", font=font(42))
    draw.text((66, 104), f"14 MYTHICAL BEASTS / 4 OFFICE TOOLS · EDITION {edition:02}", fill="#647269", font=font(21))
    draw.text((1260, 55), "HarmonyOS · tablet / 2in1", fill="#647269", font=font(22))
    draw.line((66, 154, 1734, 154), fill="#D7D7CE", width=2)
    for i, app in enumerate(apps):
        x, y = 66 + (i % 6) * 284, 190 + (i // 6) * 342
        icon = Image.open(SUITE / app["images"]["512"]).convert("RGB").resize((212, 212), Image.Resampling.LANCZOS)
        mask = Image.new("L", (212, 212))
        ImageDraw.Draw(mask).rounded_rectangle((0, 0, 211, 211), radius=46, fill=255)
        sheet.paste(icon, (x, y), mask)
        draw.text((x, y + 227), app["en"], fill="#17231E", font=font(28))
        draw.text((x, y + 266), app["name"], fill="#647269", font=font(21))
        draw.text((x, y + 298), app["symbol"], fill="#647269", font=font(17))
    sheet.save(SUITE / "overview.png", optimize=True)

    sheet = Image.new("RGB", (1500, 1510), "#F4F1EA")
    draw = ImageDraw.Draw(sheet)
    draw.text((50, 30), "ZAOYU / SIZE & MASK STUDY", fill="#17231E", font=font(34))
    draw.text((50, 83), "128 px circle  /  64 px rounded  /  48 px rounded  /  32 px square", fill="#647269", font=font(20))
    for i, app in enumerate(apps):
        x, y = 50 + (i % 3) * 490, 148 + (i // 3) * 220
        draw.text((x, y), app["en"] + " · " + app["symbol"], fill="#17231E", font=font(21))
        icon = Image.open(SUITE / app["images"]["512"]).convert("RGB")
        for size, dx, circle in ((128, 0, True), (64, 152, False), (48, 242, False), (32, 316, False)):
            small = icon.resize((size, size), Image.Resampling.LANCZOS)
            mask = Image.new("L", (size, size), 255 if size == 32 else 0)
            md = ImageDraw.Draw(mask)
            if circle:
                md.ellipse((0, 0, size - 1, size - 1), fill=255)
            elif size != 32:
                md.rounded_rectangle((0, 0, size - 1, size - 1), radius=size // 5, fill=255)
            sheet.paste(small, (x + dx, y + 45 + (128 - size) // 2), mask)
    sheet.save(SUITE / "size-and-mask-preview.png", optimize=True)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    spec = json.loads((SUITE / "redesign-spec.json").read_text())
    names = json.loads((SUITE / "names.json").read_text())
    catalog = {a["id"]: a for a in json.loads((ROOT / "config/apps.json").read_text())["apps"]}
    designs = {a["product"]: a for a in spec["apps"]}
    assert set(designs) == set(catalog), "Every application must have a design"
    assert len(names["apps"]) == len(catalog) and {a["product_slug"] for a in names["apps"]} == set(catalog)
    apps = []
    runtime_count = 0
    for app in names["apps"]:
        product = app["product_slug"]
        design, config = designs[product], catalog[product]
        master = SUITE / "icons/masters" / app["filename"]
        app.pop("animal", None)
        assert hashlib.sha256(master.read_bytes()).hexdigest() == design["master_sha256"], master
        with Image.open(master) as source:
            assert source.width == source.height and source.width >= 1024, master
            assert source.convert("RGBA").getchannel("A").getextrema() == (255, 255), master
            source = source.convert("RGB")
            app.update(master_size=list(source.size), master_sha256=hashlib.sha256(master.read_bytes()).hexdigest(),
                       bg=design["bg"], accent="#F6EDD7", motif=design["motif"], symbol=design["symbol"],
                       integrationStatus=config["status"], designVersion=spec["version"],
                       assetEdition=design["assetEdition"], theme=design["theme"],
                       pronunciation=design["pronunciation"], rationale=design["rationale"],
                       framing=design["framing"], features=design["features"], composition=design["composition"],
                       style="mythic-engraving" if design["theme"] == "中国神话异兽" else "engraved-atelier")
            for size in SIZES:
                target = SUITE / "icons" / str(size) / app["filename"]
                target.parent.mkdir(parents=True, exist_ok=True)
                framed_icon(source, size, design).save(target, optimize=True)
        large = SUITE / "icons/512" / app["filename"]
        shutil.copyfile(large, ROOT / f"AppScope/resources/base/media/{product}_icon.png")
        shutil.copyfile(large, ROOT / config["entry"] / "src/main/resources/base/media/app_icon.png")
        runtime_count += 2
        if config.get("nativeCrate"):
            shutil.copyfile(SUITE / "icons/128" / app["filename"], ROOT / f"branding/{product}-128.png")
            runtime_count += 1
        apps.append({"id": product, "n": app["n"], "en": app["en"], "name": app["name"],
                     "bundleName": config["bundleName"], "purpose": app["purpose"], "category": app["category"],
                     "symbol": app["symbol"], "bg": app["bg"], "upstream": app["old"],
                     "theme": app["theme"], "pronunciation": app["pronunciation"], "rationale": app["rationale"],
                     "features": app["features"], "composition": app["composition"],
                     "description": f"{app['name']}面向鸿蒙平板与 2in1，提供{app['purpose']}功能。",
                     "images": {str(s): f"icons/{s}/{app['filename']}" for s in SIZES}})
    names.update(date=spec["date"], repositoryEdition=spec["version"], designVersion=spec["version"], style=spec["style"])
    write_json(SUITE / "names.json", names)
    rows = [["序号", "原应用", "中文名称", "英文名称（单词式造词）", "词根", "命名联想", "英文包名 bundleName",
             "包名状态", "集成状态", "用途", "分类", "图标路径", "主色", "强调色", "图案", "读音", "设计联想", "辨识特征", "构图"]]
    rows += [[a["n"], a["old"], a["name"], a["en"], a["name_root"], a["name_rationale"], a["bundleName"],
              "工程配置，未核验商店注册", a["integrationStatus"], a["purpose"], a["category"], a["icon"], a["bg"], a["accent"], a["symbol"], a["pronunciation"], a["rationale"], a["features"], a["composition"]] for a in names["apps"]]
    write_csv(SUITE / "names.csv", rows)
    bundles = json.loads((SUITE / "bundle-names.json").read_text())
    bundles["repositoryEdition"] = spec["version"]
    for app in bundles["apps"]:
        app["integrationStatus"] = catalog[app["nameEn"].lower()]["status"]
    write_json(SUITE / "bundle-names.json", bundles)
    write_csv(SUITE / "bundle-names.csv", [["original_app", "app_name_zh", "app_name_en", "name_root", "bundle_name", "status", "integration_status"]] +
              [[a["originalApp"], a["nameZh"], a["nameEn"], a["nameRoot"], a["bundleName"], bundles["status"], a["integrationStatus"]] for a in bundles["apps"]])
    (SUITE / "bundle-names.txt").write_text(f"ZAOYU / APPLICATION IDENTITIES — {spec['date']}\nDevices: tablet, 2in1\n\n" +
        "\n".join(f"{a['n']:02}. {a['name']} | {a['en']}\n    {a['bundleName']}\n    {a['purpose']}\n" for a in apps))
    prompts = [f"造隅全系列图标 / 第 {spec['version']} 版 / {spec['date']} / 内置 imagegen", spec["generationMethod"]]
    for app in spec["apps"]:
        prompts += [f"\n{app['product']} / {app['filename']} / asset edition {app['assetEdition']}",
                    f"Framing: {json.dumps(app['framing'])}", f"Features: {app['features']}",
                    f"Image inputs: {app['imageInputs']}", app["generationPrompt"]]
        if app.get("initialGenerationPrompt"):
            prompts += ["Initial generation before refinement:", app["initialGenerationPrompt"]]
    (SUITE / "generation-prompts.txt").write_text("\n\n".join(prompts) + "\n")
    template = (ROOT / "tools/icon-gallery.html").read_text()
    template = template.replace("__DESIGN_EDITION__", f"{spec['version']:02}").replace("__DESIGN_VERSION__", str(spec["version"]))
    template = template.replace("__DESIGN_DATE__", spec["date"].replace("-", "."))
    render = lambda data: template.replace("__APP_DATA__", json.dumps(data, ensure_ascii=False).replace("</", "<\\/"))
    (SUITE / "index.html").write_text(render(apps))
    standalone = [{**a, "images": {s: data_url(SUITE / path) for s, path in a["images"].items()}} for a in apps]
    html = OUT / "zaoyu-app-icons.html"
    html.write_text(render(standalone))
    previews(apps, spec["version"])
    fields = ["中文名称", "英文名称", "包名", "用途", "简介草稿", "设备", "图案", "读音", "设计联想"]
    listing_rows = [fields] + [[a["name"], a["en"], a["bundleName"], a["purpose"], a["description"], "tablet / 2in1", a["symbol"], a["pronunciation"], a["rationale"]] for a in apps]
    write_csv(OUT / "app-listing.csv", listing_rows)
    write_json(OUT / "app-listing.json", [{k: v for k, v in a.items() if k != "images"} for a in apps])
    with zipfile.ZipFile(OUT / "zaoyu-app-icons.zip", "w", zipfile.ZIP_DEFLATED) as archive:
        for name in (html.name, "app-listing.csv", "app-listing.json"):
            archive.write(OUT / name, name)
        for app in apps:
            for size, path in app["images"].items():
                archive.write(SUITE / path, f"icons/{app['id']}/{app['id']}-{size}.png")
        for name in ("overview.png", "size-and-mask-preview.png", "redesign-spec.json", "generation-prompts.txt", "mythology-notes.md"):
            archive.write(SUITE / name, name)
        for path in sorted((SUITE / "generation").rglob("*")):
            if path.is_file():
                archive.write(path, str(path.relative_to(SUITE)))
    print(f"Exported {len(apps)} framed masters, {len(apps) * len(SIZES)} PNGs, {runtime_count} runtime resources.")
    print(f"Standalone HTML: {html} ({html.stat().st_size / 1024**2:.1f} MiB)")
    print(f"Package: {OUT / 'zaoyu-app-icons.zip'}")


if __name__ == "__main__":
    main()
