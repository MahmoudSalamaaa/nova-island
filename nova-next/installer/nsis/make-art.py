"""The installer's pictures, from Nova's own logo (build/icon.png):
  art/sidebar.bmp  164 x 314, the Welcome and Finish pages
  art/header.bmp   150 x 57, the top of the other pages
    python nova-next/installer/nsis/make-art.py
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent / 'art'
LOGO = Image.open(ROOT / 'build' / 'icon.png').convert('RGBA')
BOLD = 'C:/Windows/Fonts/segoeuib.ttf'
TEXT = 'C:/Windows/Fonts/segoeui.ttf'


def gradient(w, h, top, bottom):
    im = Image.new('RGB', (w, h))
    px = im.load()
    for y in range(h):
        t = y / max(1, h - 1)
        c = tuple(round(a + (b - a) * t) for a, b in zip(top, bottom))
        for x in range(w):
            px[x, y] = c
    return im


def glow(im, cx, cy, r, color, strength):
    layer = Image.new('RGBA', im.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    for i in range(r, 0, -2):
        a = round(strength * (1 - i / r) ** 2)
        d.ellipse((cx - i, cy - i, cx + i, cy + i), fill=(*color, a))
    return Image.alpha_composite(im.convert('RGBA'), layer)


def sidebar():
    w, h = 164, 314
    im = gradient(w, h, (16, 16, 26), (32, 22, 58))
    im = glow(im, w // 2, 108, 90, (111, 140, 255), 70)
    logo = LOGO.resize((84, 84), Image.LANCZOS)
    im.alpha_composite(logo, ((w - 84) // 2, 66))
    d = ImageDraw.Draw(im)
    f1, f2 = ImageFont.truetype(BOLD, 19), ImageFont.truetype(TEXT, 11)
    for text, font, y, fill in (('Nova Island', f1, 168, (255, 255, 255)), ('A living island for', f2, 200, (176, 180, 205)),
                                ('your Windows desktop', f2, 215, (176, 180, 205)), ('by AmerTech', f2, 284, (128, 132, 160))):
        tw = d.textlength(text, font=font)
        d.text(((w - tw) / 2, y), text, font=font, fill=fill)
    im.convert('RGB').save(OUT / 'sidebar.bmp')


def header():
    w, h = 150, 57
    im = Image.new('RGBA', (w, h), (255, 255, 255, 255))
    logo = LOGO.resize((40, 40), Image.LANCZOS)
    im.alpha_composite(logo, (w - 40 - 10, (h - 40) // 2))
    im.convert('RGB').save(OUT / 'header.bmp')


if __name__ == '__main__':
    OUT.mkdir(exist_ok=True)
    sidebar()
    header()
    print('made', [p.name for p in OUT.iterdir()])
