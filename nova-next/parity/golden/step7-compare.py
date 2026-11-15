# Settings pages: 1.7 (Electron, <page>-ref.png) against 1.8 (<page>-rust.png), same data.
#   python step7-compare.py <dir>
# Per page: size, share of pixels off by more than 2 in any channel, largest difference, and a
# <page>-diff.png that marks those pixels in red.
import sys
from pathlib import Path
from PIL import Image, ImageChops

PAGES = ['general', 'island', 'themes-page', 'companion', 'faith', 'alerts', 'connections', 'plugins', 'support', 'more']
d = Path(sys.argv[1])
worst = 0.0
for p in PAGES:
    a = Image.open(d / f'{p}-ref.png').convert('RGB')
    b = Image.open(d / f'{p}-rust.png').convert('RGB')
    if a.size != b.size:
        print(f'{p:12} size differs: ref {a.size} rust {b.size}')
        worst = 100.0
        continue
    diff = ImageChops.difference(a, b)
    px = diff.load()
    w, h = a.size
    off = 0
    mx = 0
    mark = a.copy()
    mp = mark.load()
    for y in range(h):
        for x in range(w):
            m = max(px[x, y])
            mx = max(mx, m)
            if m > 2:
                off += 1
                mp[x, y] = (255, 0, 0)
    share = 100.0 * off / (w * h)
    worst = max(worst, share)
    mark.save(d / f'{p}-diff.png')
    print(f'{p:12} {w}x{h}  off {share:6.3f}%  max {mx}')
print(f'worst page: {worst:.3f}% of pixels off by more than 2')
