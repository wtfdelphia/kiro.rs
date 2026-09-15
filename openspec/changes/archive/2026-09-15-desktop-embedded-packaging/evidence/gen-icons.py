#!/usr/bin/env python3
"""change 7 任务 1.1/1.2：应用图标生成
设计：深色圆角底 + 绿色状态点（与 change 6 托盘 Running 色 (76,175,80) 同源）。
产物：desktop/assets/icons/icon_{size}.png（1024..16）+ icon.ico（多尺寸）。
"""
from PIL import Image, ImageDraw
import os

OUT = "/home/openclaw/wtf_workspace/local/kiro.rs/desktop/assets/icons"
os.makedirs(OUT, exist_ok=True)

BG = (18, 22, 30, 255)          # 深色底
DOT = (76, 175, 80, 255)        # change 6 Running 绿
RING = (76, 175, 80, 70)        # 外圈光晕（低透明同色）

def render(size):
    """在 4x 超采样画布上绘制再降采样，得到平滑边缘"""
    ss = 4
    S = size * ss
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # 圆角矩形底（半径约 22%）
    radius = int(S * 0.22)
    d.rounded_rectangle([0, 0, S - 1, S - 1], radius=radius, fill=BG)
    cx = cy = S / 2
    # 光晕环
    d.ellipse([cx - S * 0.30, cy - S * 0.30, cx + S * 0.30, cy + S * 0.30], fill=RING)
    # 实心状态点（直径 32%）
    r = S * 0.16
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=DOT)
    return img.resize((size, size), Image.LANCZOS)

sizes = [1024, 512, 256, 128, 64, 48, 32, 16]
imgs = {}
for s in sizes:
    img = render(s)
    imgs[s] = img
    img.save(os.path.join(OUT, f"icon_{s}.png"))

# ico：多尺寸集合
imgs[256].save(
    os.path.join(OUT, "icon.ico"),
    format="ICO",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
print("generated:", sorted(os.listdir(OUT)))
