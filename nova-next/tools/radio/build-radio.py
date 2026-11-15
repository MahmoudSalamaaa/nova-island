"""Quran Radio 1.1.0 for Nova Island: the 1.0.0 stations, sorted into clear groups, and each
reciter with the surahs mp3quran.net has for him (played from the first ayah, in Mushaf order),
besides his live station.

    python nova-next/tools/radio/build-radio.py
Reads:  nova-island-plugins/quran-radio/manifest.json (1.0.0)
        nova-next/tools/radio/source/reciters.json, suwar.json
            (https://www.mp3quran.net/api/v3/reciters?language=ar and /suwar?language=ar)
Writes: nova-next/plugins/quran-radio/manifest.json (unsigned; sign.cjs makes the .nvp)
"""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
OUT = ROOT / 'nova-next' / 'plugins' / 'quran-radio' / 'manifest.json'
SURAH_HOST = 'https://cdn.mp3quran.net/'

GROUPS = ['الإذاعات', 'القرّاء', 'الأذكار والرقية', 'التفسير والسيرة والحديث', 'تلاوات متنوعة', 'ترجمات المعاني']
# 1.0.0 names that don't match mp3quran's spelling of the same reciter
MANUAL = {'أحمد العجمي': 5, 'أحمد خضر الطرابلسي': 201, 'علي الحذيفي': 74, 'أذاعة محمد أبوسنينة': 274}


def norm(s):
    s = re.sub(r'[ً-ْٰـ]', '', s)
    s = s.replace('أ', 'ا').replace('إ', 'ا').replace('آ', 'ا').replace('ة', 'ه').replace('ى', 'ي')
    s = re.sub(r'^(الشيخ|شيخ)\s+', '', s.strip())
    return re.sub(r'\s+', ' ', s)


def group_of(s):
    n = s['name']
    if s['group'] == 'إذاعات':
        return 'الإذاعات'
    if n.startswith('ترجمة'):
        return 'ترجمات المعاني'
    if any(w in n for w in ('أذكار', 'الرقية', 'السكينة', 'تكبيرات')):
        return 'الأذكار والرقية'
    if any(w in n for w in ('تفسير', 'السيرة', 'صحيح', 'رياض الصالحين', 'الشمائل', 'الفتاوى', 'قصص', 'الصحابة', 'فضل شهر', 'كتاب', 'المختصر')):
        return 'التفسير والسيرة والحديث'
    if any(w in n for w in ('العامة', 'تلاوات', 'تراتيل', 'سورة')):
        return 'تلاوات متنوعة'
    return 'القرّاء'


def best_moshaf(r):
    ok = [m for m in r['moshaf'] if m['server'].startswith(SURAH_HOST)]
    if not ok:
        return None
    # Hafs, murattal, complete first; then the most surahs
    return max(ok, key=lambda m: ('حفص' in m['name'], 'مرتل' in m['name'], m['surah_total']))


def main():
    old = json.loads((ROOT / 'nova-island-plugins' / 'quran-radio' / 'manifest.json').read_text(encoding='utf-8'))
    old = old.get('manifest', old)
    reciters = json.loads((HERE / 'source' / 'reciters.json').read_text(encoding='utf-8'))['reciters']
    by_name = {norm(r['name']): r for r in reciters}
    by_id = {r['id']: r for r in reciters}

    stations, matched = [], 0
    for s in old['stations']:
        g = group_of(s)
        out = {'id': s['id'], 'name': s['name'], 'url': s['url'], 'group': g}
        if g == 'القرّاء':
            r = by_id.get(MANUAL.get(s['name'])) or by_name.get(norm(s['name']))
            if not r:
                near = [x for k, x in by_name.items() if k in norm(s['name']) or norm(s['name']) in k]
                r = near[0] if len(near) == 1 else None
            m = r and best_moshaf(r)
            if m:
                matched += 1
                out['surahs'] = {'server': m['server'], 'rewaya': re.sub(r'\s+', ' ', m['name']).strip(),
                                 'list': sorted(int(x) for x in m['surah_list'].split(',') if x.strip())}
        stations.append(out)

    # groups in order; reciters and translations A-Z (the window sorts with the Arabic collation too)
    stations.sort(key=lambda s: (GROUPS.index(s['group']), norm(s['name']) if s['group'] in ('القرّاء', 'ترجمات المعاني') else ''))
    manifest = {**{k: v for k, v in old.items() if k != 'stations'}, 'version': '1.1.0',
                'description': "Qur'an radio: Cairo, Saudi and 140+ reciters, live or surah by surah", 'stations': stations}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    counts = {g: sum(1 for s in stations if s['group'] == g) for g in GROUPS}
    print(f'{len(stations)} stations, {matched} reciters with surahs -> {OUT}')
    print(json.dumps(counts, ensure_ascii=False))


if __name__ == '__main__':
    main()
