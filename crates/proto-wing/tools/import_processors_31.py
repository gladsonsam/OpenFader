#!/usr/bin/env python3
"""Build WING 3.1 registered processor variants from pinned 3.0.5 facts.

The common controls, selector identities, documented bounds and new models below
come from WING Remote Protocols 3.1-03, pp. 49, 53, 172–177. Live DD must
still match one complete roster and validate every value; unregistered variants
are unavailable rather than inferred. This does not query or mutate a mixer.
"""
import copy
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1] / 'src/schema'
old = json.loads((root / 'processors.json').read_text())
out = {}

def number(name, lo, hi, kind='linear float'):
    return {'name': name, 'type': kind, 'minfloat': lo, 'maxfloat': hi}
def boolean(name):
    return {'name': name, 'type': 'integer', 'minint': 0, 'maxint': 1}
def enum(name, *choices):
    return {'name': name, 'type': 'string enum', 'items': [{'item': item} for item in choices]}

common = [boolean('on'), number('mix', 0, 100), number('gain', -6, 12)]
models = {}
for name, rows in old['ch/gate'].items():
    if name not in ('76LA', 'LA'):
        models['D241G' if name == 'D241' else name] = copy.deepcopy([r for r in rows if r['name'] not in ('on', 'mix', 'gain')])
for name, rows in old['ch/dyn'].items():
    models['D241C' if name == 'D241' else name] = copy.deepcopy([r for r in rows if r['name'] not in ('on', 'mix', 'gain')])
# 3.1 exposes the model-specific WARM mix separately from the global
# processor mix. Its position-5 identity is corroborated against live DD and
# the pinned 3.0.5 WARM mix; names are not treated as generic aliases.
models['WARM'].append({**number('wmix', 0, 100), 'primary_alias': 'mix'})
# 3.1 node tables and appendix disagree on hold ranges: use their bounded union.
for name, maximum in (('GATE', 1000), ('DUCK', 4000)):
    for row in models[name]:
        if row['name'] == 'hld':
            row['minfloat'], row['maxfloat'] = 0, maximum
new = {
    'E88C': [enum('knee', 'SOFT', 'HARD'), number('thr', -10, 20), boolean('thrpull'), number('ratio', 1, 10, 'log float'), enum('att', 'SLOW', 'FAST'), number('rel', 10, 3000, 'log float')],
    'LMT': [number('tspd', 0, 1), number('trans', -10, 10), number('tgain', -12, 12), boolean('ton'), number('comp', 0, 100), number('cgain', -12, 12), boolean('con')],
    'ONEC': [number('gr', 0, 10), boolean('dag')],
}
for family in ('ch', 'aux', 'bus', 'main', 'mtx'):
    out[f'{family}/eq'] = copy.deepcopy(old[f'{family}/eq'])
    out[f'{family}/dyn'] = {m: copy.deepcopy(common + rows) for m, rows in (models | new).items()}
out['ch/gate'] = {m: copy.deepcopy(common + rows) for m, rows in (models | new).items()}
deq = []
for i in (1, 2):
    key = f'{i}-'
    # The 3.1 manual lists strings; live DD may report the same choices as
    # native float-enum values. Register exactly those documented numbers.
    ratio = enum(key+'ratio', '1.20', '1.30', '1.50', '2.00', '3.00', '5.00', '10.00')
    ratio['float_items'] = [1.2, 1.3, 1.5, 2.0, 3.0, 5.0, 10.0]
    deq += [number(key+'thr', -60, 0), ratio, number(key+'att', 0, 200), number(key+'rel', 20, 4000, 'log float'), enum(key+'filt', 'OFF', 'BP', 'LP6', 'LP12', 'HP6', 'HP12'), number(key+'g', -15, 15), number(key+'f', 20, 20000, 'log float'), number(key+'q', .44, 10, 'log float'), enum(key+'mode', 'low', 'high')]
for key in ('ch/gate', 'ch/dyn', 'bus/dyn', 'main/dyn', 'mtx/dyn'):
    out[key]['DEQ2'] = copy.deepcopy(common + deq)
for name in ('DS902', 'DEQ', 'WARM'):
    out['aux/dyn'].pop(name, None)
# Accept either independently documented identifier, never an arbitrary leaf.
for entries in out.values():
    for model, old_name, new_name in (('NSTR', 'out', 'ou'), ('2250', 'type', 'Type')):
        if model in entries:
            base = entries[model]
            alternative = copy.deepcopy(base)
            for row in alternative:
                if row['name'] == old_name:
                    row['name'] = new_name
            entries[model] = {'variants': [base, alternative]}
(root / 'processors_31.json').write_text(json.dumps(out, separators=(',', ':'), ensure_ascii=False))
