"""Generate the reproducible, static Arabic editor UI fixture and its assets."""

from __future__ import annotations

import json
import math
import struct
import uuid
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ASSET_DIR = ROOT / "assets" / "samples" / "m0-editor-ui"
FIXTURE = ROOT / "tests" / "fixtures" / "m0-editor-ui-v2.json"
NS = uuid.UUID("68c2b223-75a3-4fb0-9413-264a1e73d441")
BLUE = [55, 112, 238, 255]
INK = [39, 44, 54, 255]
MUTED = [119, 127, 140, 255]
LINE = [226, 229, 235, 255]
WHITE = [255, 255, 255, 255]
BG = [244, 246, 249, 255]

nodes: list[dict] = []
assets: list[dict] = []
children_by_parent: dict[str, list[str]] = {}


def ident(name: str) -> str:
    return str(uuid.uuid5(NS, name))


def props(x: float, y: float, w: float, h: float, fill=None, **kind_props) -> dict:
    value = {
        "transform": [1, 0, 0, 1, x, y],
        "size": [w, h],
        "width": "fixed",
        "height": "fixed",
    }
    if fill is not None:
        value["fill"] = fill
    value.update(kind_props)
    return value


def add(name: str, kind: str, x: float, y: float, w: float, h: float,
        fill=None, parent: str | None = None, children=None, **detail) -> str:
    node_id = ident("node/" + name)
    node = {"id": node_id, "kind": kind, "name": name, "props": props(x, y, w, h, fill, **detail)}
    if children:
        node["children"] = [ident("node/" + child) for child in children]
    nodes.append(node)
    if parent is not None:
        children_by_parent.setdefault(parent, []).append(node_id)
    return node_id


def rect(name, x, y, w, h, color, radius=0, parent=None, stroke=None):
    detail = {"shape": {"geometry": "rect", "corner_radius": radius}}
    if stroke:
        detail["stroke"] = {"color": stroke, "width": 1}
    return add(name, "shape", x, y, w, h, color, parent, **detail)


def text(name, value, x, y, w, h, size=13, color=INK, family="Noto Sans Arabic",
         weight=400, align="start", parent=None, direction="auto"):
    return add(name, "text", x, y, w, h, color, parent,
               text={"content": value, "font_family": family, "font_size": size,
                     "font_weight": weight, "direction": direction, "align": align})


def group(name, x, y, w, h, kind="group", parent=None, children=None):
    if kind == "frame":
        detail = {"frame": {"layout": "none"}}
    else:
        detail = {}
    return add(name, kind, x, y, w, h, None, parent, children, **detail)


def svg_icon(name: str, path: str) -> str:
    aid = ident("asset/" + name)
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" '
           f'viewBox="0 0 24 24"><path d="{path}" fill="none" stroke="#596273" '
           'stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg>')
    (ASSET_DIR / f"{name}.svg").write_text(svg + "\n", encoding="utf-8", newline="\n")
    assets.append({"id": aid, "name": f"{name}.svg"})
    return aid


def png_chunk(kind: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)


def make_product_png(path: Path, width=280, height=124) -> None:
    """Draw a deterministic product card illustration with only stdlib."""
    rows = []
    for y in range(height):
        row = bytearray([0])
        for x in range(width):
            # Warm, pale background with a gentle blue corner glow.
            glow = max(0.0, 1.0 - math.hypot(x - 225, y - 12) / 150)
            r, g, b = (int(246 - 8 * glow), int(247 - 1 * glow), int(250 + 4 * glow))
            # soft shadow under the charging case
            if 89 <= x <= 203 and 99 <= y <= 107:
                r, g, b = 218, 222, 231
            # white rounded charging case silhouette
            dx = max(0, 145 - x, x - 235)
            dy = max(0, 38 - y, y - 96)
            if dx * dx + dy * dy <= 13 * 13:
                r, g, b = 255, 255, 255
            if 145 <= x <= 235 and 42 <= y <= 96:
                r, g, b = 255, 255, 255
            # lid and seam
            if 151 <= x <= 229 and 56 <= y <= 59:
                r, g, b = 225, 230, 239
            # small blue status light
            if (x - 190) ** 2 + (y - 82) ** 2 <= 3 ** 2:
                r, g, b = 72, 133, 239
            # two dark earbuds, each a rounded head and stem
            for cx in (166, 214):
                if (x - cx) ** 2 + (y - 30) ** 2 <= 8 ** 2:
                    r, g, b = 53, 61, 75
                if cx - 3 <= x <= cx + 3 and 32 <= y <= 51:
                    r, g, b = 53, 61, 75
            row.extend((r, g, b, 255))
        rows.append(bytes(row))
    raw = b"".join(rows)
    data = b"\x89PNG\r\n\x1a\n"
    data += png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    data += png_chunk(b"IDAT", zlib.compress(raw, level=9)) + png_chunk(b"IEND", b"")
    path.write_bytes(data)


ASSET_DIR.mkdir(parents=True, exist_ok=True)
icons = {
    "select": "M5 3l13 10-6 1.5L9 20 5 3z",
    "frame": "M4 4h16v16H4z M8 8h8v8H8z",
    "rectangle": "M4 6h16v12H4z",
    "text": "M5 6h14 M12 6v13 M8 19h8",
    "pen": "M4 20l4-1 11-11-3-3L5 16l-1 4z M14 7l3 3",
    "share": "M12 15V3 M7 8l5-5 5 5 M5 13v7h14v-7",
    "preview": "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
}
icon_ids = {name: svg_icon(name, path) for name, path in icons.items()}
product_path = ASSET_DIR / "product.png"
make_product_png(product_path)
product_id = ident("asset/product")
assets.append({"id": product_id, "name": "product.png"})

# Root and top-level fixed panels.
root = add("واجهة المحرر", "frame", 0, 0, 1440, 900, BG,
           frame={"layout": "none"})
rect("الشريط العلوي", 0, 0, 1440, 56, WHITE, parent=root)
rect("فاصل الشريط العلوي", 0, 55, 1440, 1, LINE, parent=root)
text("اسم الملف", "شاشة الدفع — مسودة", 1180, 17, 226, 24, 14, weight=600, align="end", parent=root)
text("النسخة", "النسخة ٣", 1090, 18, 72, 22, 12, MUTED, align="end", parent=root)
rect("زر مشاركة", 986, 10, 82, 36, BLUE, 6, parent=root)
text("نص مشاركة", "مشاركة", 996, 18, 62, 20, 12, WHITE, weight=600, align="center", parent=root)
rect("زر معاينة", 896, 10, 78, 36, WHITE, 6, parent=root, stroke=LINE)
share_icon = add("أيقونة مشاركة", "image", 1020, 16, 20, 20, parent=root, image={"asset": icon_ids["share"]})
preview_icon = add("أيقونة معاينة", "image", 906, 16, 20, 20, parent=root, image={"asset": icon_ids["preview"]})
text("نص معاينة", "معاينة", 929, 18, 38, 20, 12, INK, align="center", parent=root)

# Properties inspector, intentionally on the left for this RTL sample brief.
rect("لوحة الخصائص", 0, 56, 280, 844, WHITE, parent=root)
rect("فاصل الخصائص", 279, 56, 1, 844, LINE, parent=root)
text("عنوان الخصائص", "الخصائص", 20, 75, 240, 24, 16, weight=700, parent=root)
text("العنصر المحدد", "إطار  ·  شاشة الدفع", 20, 110, 240, 20, 12, MUTED, parent=root)
rect("فاصل القسم الموضع", 16, 145, 248, 1, LINE, parent=root)
text("عنوان الموضع", "الموضع", 20, 159, 240, 22, 13, weight=600, parent=root)
for label, x, value in [("X", 20, "248"), ("Y", 146, "128")]:
    text("حقل " + label, label, x, 190, 24, 18, 11, MUTED, family="Inter", parent=root)
    rect("إدخال " + label, x, 210, 112, 34, BG, 4, parent=root, stroke=LINE)
    text("قيمة " + label, value, x + 9, 218, 94, 18, 12, INK, family="Inter", align="end", parent=root)
text("العرض", "العرض", 20, 263, 100, 20, 12, MUTED, parent=root)
text("الارتفاع", "الارتفاع", 146, 263, 100, 20, 12, MUTED, parent=root)
rect("إدخال العرض", 20, 285, 112, 34, BG, 4, parent=root, stroke=LINE)
rect("إدخال الارتفاع", 146, 285, 112, 34, BG, 4, parent=root, stroke=LINE)
text("قيمة العرض", "390", 29, 293, 94, 18, 12, INK, family="Inter", align="end", parent=root)
text("قيمة الارتفاع", "844", 155, 293, 94, 18, 12, INK, family="Inter", align="end", parent=root)
rect("فاصل القسم المظهر", 16, 337, 248, 1, LINE, parent=root)
text("عنوان التعبئة", "التعبئة", 20, 352, 100, 20, 13, weight=600, parent=root)
rect("لون التعبئة", 20, 382, 22, 22, [248, 249, 251, 255], 4, parent=root, stroke=LINE)
text("قيمة التعبئة", "#F8F9FB", 52, 384, 150, 18, 12, INK, family="Inter", parent=root)
text("عنوان الحدود", "الحدود", 20, 423, 100, 20, 13, weight=600, parent=root)
rect("لون الحدود", 20, 453, 22, 22, [214, 219, 228, 255], 4, parent=root, stroke=LINE)
text("قيمة الحدود", "#D6DBE4  ·  1", 52, 455, 170, 18, 12, INK, family="Inter", parent=root)
rect("فاصل القسم التخطيط", 16, 493, 248, 1, LINE, parent=root)
text("عنوان التخطيط", "التخطيط التلقائي", 20, 508, 220, 22, 13, weight=600, parent=root)
text("نوع التخطيط", "إطار — Auto Layout · Frame", 20, 540, 230, 18, 12, MUTED, parent=root)
text("عنوان المسافة", "المسافة بين العناصر", 20, 576, 220, 22, 12, INK, parent=root)
rect("إدخال المسافة", 20, 606, 112, 34, BG, 4, parent=root, stroke=LINE)
text("قيمة المسافة", "16", 29, 614, 94, 18, 12, INK, family="Inter", align="end", parent=root)

# Toolbar and central work area.
rect("شريط الأدوات", 296, 72, 48, 276, WHITE, 8, parent=root, stroke=LINE)
toolbar = [("تحديد", "select"), ("إطار", "frame"), ("مستطيل", "rectangle"), ("نص", "text"), ("قلم", "pen")]
for i, (label, key) in enumerate(toolbar):
    yy = 86 + i * 50
    if i == 0:
        rect("تمييز أداة " + label, 302, yy - 2, 36, 38, [232, 240, 255, 255], 6, parent=root)
    add("أيقونة أداة " + label, "image", 310, yy + 5, 22, 22, parent=root,
        image={"asset": icon_ids[key]})
    text("تسمية أداة " + label, label, 350, yy + 5, 62, 22, 12, INK, parent=root)

rect("منطقة العمل", 365, 72, 705, 790, [237, 240, 245, 255], 8, parent=root)
text("اسم الصفحة", "الصفحة ١", 392, 88, 140, 20, 12, MUTED, parent=root)
text("تكبير اللوحة", "100%", 1000, 88, 42, 18, 11, MUTED, family="Inter", align="end", parent=root)
# Phone screen mockup inside the canvas.
phone_scene = group("مشهد شاشة الهاتف", 0, 0, 1440, 900, parent=root)
rect("ظل الهاتف", 573, 150, 296, 632, [224, 228, 235, 255], 28, parent=phone_scene)
rect("إطار الهاتف", 568, 144, 296, 632, WHITE, 26, parent=phone_scene, stroke=[213, 218, 227, 255])
rect("شاشة الهاتف", 578, 154, 276, 612, [250, 251, 253, 255], 20, parent=phone_scene)
text("وقت الهاتف", "9:41", 598, 170, 52, 18, 11, INK, family="Inter", weight=600, parent=phone_scene)
text("عنوان الدفع", "إتمام الشراء", 598, 210, 236, 36, 20, INK, weight=700, align="end", parent=phone_scene)
text("وصف الدفع", "راجع طلبك وأكمل عملية الدفع", 598, 252, 236, 22, 12, MUTED, align="end", parent=phone_scene)
rect("بطاقة المنتج", 594, 296, 244, 188, WHITE, 10, parent=phone_scene, stroke=LINE)
add("صورة المنتج", "image", 606, 308, 220, 104, parent=phone_scene, image={"asset": product_id})
text("اسم المنتج", "سماعات لاسلكية", 608, 421, 210, 24, 15, INK, weight=600, align="end", parent=phone_scene)
text("سعر المنتج", "349 ر.س", 608, 452, 210, 20, 12, MUTED, align="end", parent=phone_scene)
text("عنوان طريقة الدفع", "طريقة الدفع", 598, 513, 236, 22, 13, INK, weight=600, align="end", parent=phone_scene)
rect("بطاقة البطاقة البنكية", 594, 543, 244, 48, WHITE, 8, parent=phone_scene, stroke=LINE)
text("رقم البطاقة", "••••  ••••  ••••  ٤٢٨١", 608, 556, 210, 20, 12, INK, family="Noto Sans Arabic", align="end", parent=phone_scene)
rect("زر تأكيد الدفع", 594, 704, 244, 46, BLUE, 8, parent=phone_scene)
text("نص تأكيد الدفع", "ادفع الآن", 604, 715, 224, 22, 14, WHITE, weight=700, align="center", parent=phone_scene)

# Right layer tree and assistant panel.
rect("لوحة الطبقات", 1090, 56, 350, 440, WHITE, parent=root)
rect("فاصل الطبقات", 1089, 56, 1, 844, LINE, parent=root)
text("عنوان الطبقات", "الطبقات", 1110, 75, 300, 24, 16, weight=700, parent=root)
text("اسم الصفحة في الشجرة", "الصفحة ١", 1110, 112, 286, 20, 12, MUTED, parent=root)
layer_tree = group("شجرة الطبقات", 0, 0, 1440, 900, parent=root)
text("طبقة شاشة الدفع", "▾  شاشة الدفع", 1120, 150, 280, 22, 13, INK, weight=600, parent=layer_tree)
text("طبقة شريط التنقل", "　▾  شريط التنقل", 1140, 184, 260, 22, 12, INK, parent=layer_tree)
text("طبقة إتمام الشراء", "　　T  إتمام الشراء", 1162, 216, 238, 22, 12, MUTED, parent=layer_tree)
text("طبقة بطاقة المنتج", "　▱  بطاقة المنتج", 1140, 252, 260, 22, 12, INK, parent=layer_tree)
text("طبقة صورة المنتج", "　▧  صورة المنتج", 1140, 286, 260, 22, 12, INK, parent=layer_tree)
rect("فاصل مساعد الذكاء", 1090, 496, 350, 1, LINE, parent=root)
text("عنوان المساعد", "المساعد الذكي", 1110, 514, 300, 24, 15, weight=700, parent=root)
rect("بطاقة اقتراح المساعد", 1106, 552, 318, 254, [250, 251, 253, 255], 10, parent=root, stroke=LINE)
text("موجه المساعد", "اطلب من المساعد", 1122, 570, 286, 20, 12, MUTED, parent=root)
rect("حقل طلب المساعد", 1120, 599, 290, 62, WHITE, 7, parent=root, stroke=LINE)
text("طلب المساعد", "اجعل الزر أكبر وغيّر لونه إلى الأخضر", 1132, 610, 266, 42, 12, INK, parent=root)
rect("ملخص تعديل", 1120, 675, 290, 46, [237, 244, 255, 255], 7, parent=root)
text("نتيجة المساعد", "تم تعديل ٣ طبقات", 1132, 688, 266, 20, 12, BLUE, weight=600, parent=root)
rect("زر رفض", 1120, 739, 80, 36, WHITE, 6, parent=root, stroke=LINE)
text("نص رفض", "رفض", 1128, 747, 64, 20, 12, INK, weight=600, align="center", parent=root)
rect("زر قبول", 1318, 739, 92, 36, BLUE, 6, parent=root)
text("نص قبول", "قبول", 1326, 747, 76, 20, 12, WHITE, weight=600, align="center", parent=root)

fixture = {
    "schema_version": 2,
    "id": ident("document/m0-editor-ui"),
    "pages": [{"id": ident("page/m0-editor-ui"), "name": "واجهة المحرر", "roots": [root]}],
    "assets": assets,
    "nodes": nodes,
}
for node in nodes:
    if node["id"] in children_by_parent:
        node["children"] = children_by_parent[node["id"]]
FIXTURE.write_text(json.dumps(fixture, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
print(f"wrote {FIXTURE.relative_to(ROOT)} ({len(nodes)} nodes, {len(assets)} assets)")
