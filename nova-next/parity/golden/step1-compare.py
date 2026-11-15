"""Step 1 exit check: does the new app render the unchanged island exactly like Chromium does today?

For each state, the same staged page (promo/src/island.html with mock.js, pre.js and drive.js, the
promo glass image as the backdrop) is rendered twice at 640x320:
  reference  the repo's own Electron 44 (electron-capture.js): same engine and GPU path as 1.7
  new        nova-island.exe --mock (WebView2 inside the 1.8 app), captured with CapturePreview
and the two images are compared pixel by pixel.

    python nova-next/parity/golden/step1-compare.py [state ...]
Output: <target>/golden/<state>-{ref,new,diff}.png and a summary table.
"""
import subprocess
import sys
from pathlib import Path
import os
TARGET = Path(os.environ.get('CARGO_TARGET_DIR') or Path(__file__).resolve().parents[2] / 'target')

from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'promo' / 'src'))
import build  # noqa: E402  (the promo pipeline: staging, local server, Edge path)
import check  # noqa: E402  (per-state time budgets)

APP = TARGET / ('debug/nova-island.exe')
OUT = TARGET / ('golden')
STATES = ['idle', 'music', 'compact', 'apps', 'system', 'prayer', 'shelf', 'downloads', 'reminder', 'volume']
BD = 'promo/src/out/01-hero-bd0.png'
# random content is pinned so both sides show the same thing (rm: the reminder, by its reference)
EXTRA = {'reminder': '&rm=%D8%A7%D9%84%D8%A8%D9%82%D8%B1%D8%A9%20%D9%A2%D9%A1%D9%A4'}


ELECTRON = ROOT / 'node_modules' / 'electron' / 'dist' / 'electron.exe'


def references(states):
    jobs = [f'{st}{EXTRA.get(st, "")}={check.BUDGET.get(st, 6000) + 800}' for st in states]
    subprocess.run([str(ELECTRON), str(Path(__file__).with_name('electron-capture.js')), str(build.PORT), str(OUT), *jobs],
                   check=True, capture_output=True, timeout=600)


def new(state, out, budget):
    # bottom-left of the screen: a window that's off screen may not draw at all
    r = subprocess.run([str(APP), '--mock', f'state={state}{EXTRA.get(state, "")}&bd={BD}', '--bg', '1a1a24', '--pos', '0,600',
                        '--exit-after', str(budget + 800), '--capture', str(out)], capture_output=True, text=True, timeout=120)
    errors = [l for l in r.stderr.splitlines() if l.startswith('[renderer]')]
    return errors


def compare(a, b, diff_out):
    ia, ib = Image.open(a).convert('RGB'), Image.open(b).convert('RGB')
    if ia.size != ib.size:
        return {'size': f'{ia.size} vs {ib.size}'}
    d = ImageChops.difference(ia, ib)
    px = list(d.get_flattened_data()) if hasattr(d, 'get_flattened_data') else list(d.getdata())
    n = len(px)
    m = [max(p) for p in px]
    over8 = sum(1 for v in m if v > 8) / n * 100
    over32 = sum(1 for v in m if v > 32) / n * 100
    # heat map: white = identical, red = different
    heat = Image.new('RGB', ia.size, 'white')
    heat.putdata([(255, 255 - min(255, v * 4), 255 - min(255, v * 4)) for v in m])
    side = Image.new('RGB', (ia.size[0] * 3 + 20, ia.size[1]), 'white')
    side.paste(ia, (0, 0))
    side.paste(ib, (ia.size[0] + 10, 0))
    side.paste(heat, (ia.size[0] * 2 + 20, 0))
    side.save(diff_out)
    return {'identical': max(m) == 0, 'over8': round(over8, 3), 'over32': round(over32, 3), 'max': max(m)}


def main():
    states = sys.argv[1:] or STATES
    OUT.mkdir(parents=True, exist_ok=True)
    build.WORK.mkdir(exist_ok=True)
    build.stage_page('index.html', 'island.html', ['promo/src/mock.js', 'promo/src/pre.js'], ['promo/src/drive.js'], '../../')
    httpd = build.serve()
    rows = []
    try:
        references(states)
        for st in states:
            ref, got, diff = OUT / f'{st}-ref.png', OUT / f'{st}-new.png', OUT / f'{st}-diff.png'
            errors = new(st, got, check.BUDGET.get(st, 6000))
            res = compare(ref, got, diff)
            rows.append((st, res, errors))
            print(f'{st:10} {res}' + (f'  page errors: {errors}' if errors else ''))
    finally:
        httpd.shutdown()
    bad = [st for st, res, _ in rows if not res.get('identical')]
    print(f'\n{len(rows) - len(bad)}/{len(rows)} identical' + (f'; different: {", ".join(bad)}' if bad else ''))


if __name__ == '__main__':
    main()
