#!/usr/bin/env python3
"""Record model-position-5 hashes from the pinned libwing 3.0.5 propmap."""
import json
from pathlib import Path
import sys
rows = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines()]
out = {}
for family, domain, count in [('ch','gate',40),('ch','dyn',40),('bus','dyn',16),('main','dyn',4),('mtx','dyn',8)]:
    for index in range(1, count + 1):
        base = f'/{family}/{index}/{domain}'
        ids = {r['id'] for r in rows if r['fullname'].startswith(base + '/') and len(r['fullname'].split('/')) == 6 and r.get('index') == 5}
        if len(ids) != 1:
            raise ValueError(f'position 5 has ambiguous hash for {base}')
        out[base] = ids.pop()
Path(__file__).resolve().parents[1].joinpath('src/schema/warm_position5_ids.json').write_text(json.dumps(out, separators=(',', ':'), sort_keys=True))
