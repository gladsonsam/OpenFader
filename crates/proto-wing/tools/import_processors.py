#!/usr/bin/env python3
"""Extract WING processor roster facts from the pinned libwing propmap.jsonl."""
import json
from pathlib import Path
import sys

rows = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines()]
result = {}
for family in ('ch', 'aux', 'bus', 'main', 'mtx'):
    for domain in ('gate', 'dyn', 'eq'):
        prefix = f'/{family}/1/{domain}/'
        scoped = [r for r in rows if r['fullname'].startswith(prefix)]
        selector = next((r for r in scoped if r['fullname'] == prefix + 'mdl'), None)
        if selector is None:
            continue
        common = [r for r in scoped if r['fullname'] in [prefix + n for n in ('on', 'mix', 'gain')]]
        models = {}
        for choice in selector['items']:
            model = choice['item']
            leaves = [r for r in scoped if r['fullname'].startswith(prefix + model + '/') and len(r['fullname'].split('/')) == 6]
            if leaves:
                models[model] = [{k: r[k] for k in ('name', 'type', 'minfloat', 'maxfloat', 'minint', 'maxint', 'items') if k in r} for r in common + leaves if not r['name'].startswith('$')]
        result[f'{family}/{domain}'] = models
Path(__file__).resolve().parents[1].joinpath('src/schema/processors.json').write_text(json.dumps(result, separators=(',', ':'), ensure_ascii=False))
