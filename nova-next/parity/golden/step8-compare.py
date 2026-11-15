"""Step 8: the island in 1.7 and 1.8, picture by picture.

The same staged page as step 1 (promo/src/island.html with the promo drive.js), for:
  every state drive.js knows, in the default theme
  every theme, for the states the theme changes most (idle pill, music, prayer)
  vertical, docked right and left, for four states
Each one is rendered by the repo's Electron (reference) and by nova-island.exe --mock (1.8), and
compared pixel by pixel.

    python nova-next/parity/golden/step8-compare.py
Output: <target>/golden8/<name>-{ref,new,diff}.png, results.json and a summary.
"""
import importlib.util
import json
import re
import subprocess
import sys
import time
from pathlib import Path
import os
TARGET = Path(os.environ.get('CARGO_TARGET_DIR') or Path(__file__).resolve().parents[2] / 'target')

_spec = importlib.util.spec_from_file_location('step1', Path(__file__).with_name('step1-compare.py'))
s1 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(s1)

ROOT = s1.ROOT
OUT = TARGET / ('golden8')
DRIVE = (ROOT / 'promo' / 'src' / 'drive.js').read_text(encoding='utf-8')
STATES = ['idle'] + re.findall(r"case '([a-z0-9-]+)'", DRIVE)
THEMES = ['bakka', 'outline', 'midnight', 'forest', 'snow', 'cat', 'galaxy', 'aurora', 'ocean', 'nody', 'lava', 'retro']
THEMED = ['idle', 'music', 'prayer']
VERTICAL = ['idle', 'music', 'prayer', 'downloads']


def jobs():
    out = [(st, st) for st in STATES]
    out += [(f'{st}-{th}', f'{st}&theme={th}') for th in THEMES for st in THEMED]
    out += [(f'{st}-v{side[0]}', f'{st}&v=1&side={side}') for side in ('right', 'left') for st in VERTICAL]
    return out


def budget(name):
    return s1.check.BUDGET.get(name.split('&')[0], 6000)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    s1.OUT = OUT
    s1.build.WORK.mkdir(exist_ok=True)
    s1.build.stage_page('index.html', 'island.html', ['promo/src/mock.js', 'promo/src/pre.js'], ['promo/src/drive.js'], '../../')
    httpd = s1.build.serve()
    todo = jobs()
    results = {}
    t0 = time.time()
    try:
        refs = [f'{q}{s1.EXTRA.get(q.split("&")[0], "")}&n={n}={budget(q) + 800}' for n, q in todo]
        subprocess.run([str(s1.ELECTRON), str(Path(__file__).with_name('electron-capture.js')), str(s1.build.PORT), str(OUT), *refs],
                       check=True, capture_output=True, timeout=1800)
        print(f'references: {time.time() - t0:.0f} s', flush=True)
        for n, q in todo:
            ref, got, diff = OUT / f'{n}-ref.png', OUT / f'{n}-new.png', OUT / f'{n}-diff.png'
            if not ref.exists():
                results[n] = {'error': 'no reference'}
                continue
            errors = s1.new(q, got, budget(q))
            results[n] = s1.compare(ref, got, diff) if got.exists() else {'error': 'no capture'}
            if errors:
                results[n]['page_errors'] = errors
            print(f'{n:22} {results[n]}', flush=True)
    finally:
        httpd.shutdown()
    (OUT / 'results.json').write_text(json.dumps(results, indent=1), encoding='utf-8')
    same = [n for n, r in results.items() if r.get('identical')]
    print(f'\n{len(same)}/{len(results)} identical, {time.time() - t0:.0f} s')


if __name__ == '__main__':
    main()
