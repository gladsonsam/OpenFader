#!/usr/bin/env python3
"""Offline integration inventory. Wiring is source evidence, never support proof."""
import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
STATES = {'unreviewed', 'missing', 'partial', 'implemented', 'blocked', 'not_applicable'}
# `ui` is manual visual verification on any recorded platform. `ipad` and `iphone`
# stay accepted only for claims specifically about iPadOS/iOS behaviour.
KINDS = {'unit', 'emulator', 'hardware', 'ui', 'ipad', 'iphone'}


def load(path):
    return json.loads(path.read_text())


def document(path):
    value = load(path)
    if value.get('schema') != 1:
        raise ValueError(f'Unsupported integration schema: {path}')
    return value


def masked_rust(text):
    """Mask comments/literals without changing offsets; keep lifetimes intact."""
    pattern = r'//[^\n]*|/\*|r(\#*)"|"|\x27(?:\\.|[^\x27\\\n])\x27'
    result = list(text)
    position = 0
    while match := re.search(pattern, text[position:]):
        start = position + match.start()
        token = match.group()
        end = position + match.end()
        if token == '/*':
            depth = 1
            while depth and end < len(text):
                if text.startswith('/*', end):
                    depth += 1
                    end += 2
                elif text.startswith('*/', end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            if depth:
                raise ValueError('Unterminated Rust block comment')
        elif token.startswith('r'):
            closing = '"' + token[1:-1]
            found = text.find(closing, end)
            if found < 0:
                raise ValueError('Unterminated Rust raw string')
            end = found + len(closing)
        elif token == '"':
            while end < len(text):
                if text[end] == '\\':
                    end += 2
                elif text[end] == '"':
                    end += 1
                    break
                else:
                    end += 1
        for index in range(start, end):
            if text[index] != '\n':
                result[index] = ' '
        position = end
    return ''.join(result)


def regions(text, pattern):
    masked = masked_rust(text)
    for match in re.finditer(pattern, masked):
        start = masked.index('{', match.start()) + 1
        depth, end = 1, start
        while depth and end < len(masked):
            depth += (masked[end] == '{') - (masked[end] == '}')
            end += 1
        if depth:
            raise ValueError('Unbalanced Rust block')
        yield match, start, end - 1


def methods(text):
    masked = masked_rust(text)
    depth, last = 0, 0
    found = {}
    for match in re.finditer(r'\b(?:async\s+)?fn\s+(\w+)\b', masked):
        for ch in masked[last:match.start()]:
            depth += (ch == '{') - (ch == '}')
        if depth == 0:
            found[match[1]] = text[:match.start()].count('\n') + 1
        last = match.start()
    return found


def source_inventory(root):
    path = root / 'crates/engine/src/drivers.rs'
    source = path.read_text()
    registered = set()
    for _, start, end in regions(source, r'\bRegisteredDriver\s*\{'):
        if match := re.search(r'\bname:\s*"([\w-]+)"', source[start:end]):
            registered.add(match[1])
    if not registered:
        raise ValueError('Cannot read driver registry; source scanner needs updating')
    trait = list(regions(source, r'\bpub\s+trait\s+DynDriver\s*[^\{]*\{'))
    if len(trait) != 1:
        raise ValueError('Cannot identify the DynDriver contract')
    _, start, end = trait[0]
    contract = set(methods(source[start:end]))
    implementations = {}
    paths = [path, *sorted((root / 'crates/engine/src/drivers').glob('*.rs'))]
    for file in paths:
        text = file.read_text()
        for match, start, end in regions(text, r'impl_dyn_driver!\s*\(\s*([\w:]+)\s*\{'):
            key = match[1]
            if key in implementations:
                raise ValueError(f'Duplicate driver adapter {key}')
            implementations[key] = {
                name: {'path': str(file.relative_to(root)),
                       'line': text[:start].count('\n') + line}
                for name, line in methods(text[start:end]).items()
            }
    return registered, contract, implementations


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def references(root, paths):
    for path in paths:
        resolved = (root / path).resolve()
        if not resolved.is_relative_to(root.resolve()) or not resolved.is_file():
            raise ValueError(f'Missing or unsafe source reference: {path}')


def evidence_for(root, records, model, feature, paths):
    result = {}
    for record in records:
        if record.get('model') != model or record.get('feature') != feature['id']:
            continue
        kind = record.get('kind')
        if kind not in KINDS or record.get('result') not in {'pass', 'fail', 'blocked'}:
            raise ValueError(f'Invalid evidence record for {model}:{feature["id"]}')
        if kind == 'hardware' and not record.get('firmware'):
            raise ValueError('Hardware evidence requires model firmware (or explicit OS/audio driver versions for host audio)')
        if kind == 'ui' and not record.get('platform'):
            raise ValueError('UI evidence requires the platform used for manual visual verification')
        if kind in {'ipad', 'iphone'} and not record.get('device'):
            raise ValueError('Simulator evidence requires device identity')
        stamp = dt.datetime.fromisoformat(record.get('recorded_at', '').replace('Z', '+00:00'))
        if stamp.tzinfo is None or stamp > dt.datetime.now(dt.timezone.utc):
            raise ValueError('Evidence needs a timezone-aware, non-future recorded_at')
        artifact = Path(record['artifact'])
        if not artifact.is_absolute():
            artifact = root / artifact
        if not artifact.is_file() or digest(artifact) != record.get('sha256'):
            raise ValueError(f'Evidence artifact missing or checksum differs: {artifact}')
        fingerprints = record.get('source_sha256', {})
        current = all(fingerprints.get(p) == digest(root / p) for p in paths)
        checks = record['result'] != 'pass' or set(feature['acceptance']) <= set(record.get('checks', []))
        status = record['result'] if current and checks else 'stale'
        # The latest explicit result wins; an old pass must not hide a failure.
        if kind not in result or stamp > dt.datetime.fromisoformat(result[kind]['recorded_at'].replace('Z', '+00:00')):
            result[kind] = {**record, 'status': status}
    return result


def build(root=ROOT, evidence=()):
    catalog = document(root / 'scripts/mixer-lab/integration_features.json')
    lab = load(root / 'scripts/mixer-lab/matrix.json')
    targets = document(root / 'scripts/mixer-lab/integration_targets.json')
    registered, contract, implementations = source_inventory(root)
    features = catalog['features']
    feature_ids = [f['id'] for f in features]
    if len(set(feature_ids)) != len(feature_ids):
        raise ValueError('Duplicate feature ID')
    classified = {name for f in features for name in f.get('engine_contracts', [])}
    if contract != classified:
        raise ValueError(f'Engine contracts need feature classification: new={sorted(contract-classified)}, removed={sorted(classified-contract)}')
    for feature in features:
        if feature['scope'] not in {'provider', 'host'} or not feature['acceptance']:
            raise ValueError(f'Invalid feature contract {feature["id"]}')
        if not set(feature.get('required_hooks', [])) <= set(feature.get('engine_contracts', [])):
            raise ValueError(f'Required hooks must belong to {feature["id"]}')
        references(root, feature['shared_paths'])
    manifests = []
    for path in sorted((root / 'crates').glob('*/integration.json')):
        manifests.append({**document(path), 'manifest_path': str(path.relative_to(root))})
    provider_ids = [p['id'] for p in manifests if p['scope'] == 'provider']
    if len(provider_ids) != len(set(provider_ids)) or set(provider_ids) != registered:
        raise ValueError(f'Registry/catalogue mismatch: registered={sorted(registered)}, manifests={sorted(provider_ids)}')
    known_types = {p['adapter'] for p in manifests if p['scope'] == 'provider'}
    if known_types != set(implementations):
        raise ValueError('Driver adapters changed; update their owning integration manifests')
    cells, models = [], []
    for provider in manifests:
        scoped = [f for f in features if f['scope'] == provider['scope']]
        if set(provider['features']) != {f['id'] for f in scoped}:
            raise ValueError(f'{provider["id"]}: every feature needs an explicit disposition; new columns cannot be skipped')
        references(root, provider['source_paths'])
        owned = set(provider['source_paths']) | {provider['manifest_path'], 'scripts/mixer-lab/integration_features.json'}
        for pattern in provider.get('source_globs', []):
            if '..' in Path(pattern).parts or Path(pattern).is_absolute():
                raise ValueError('Source globs must stay inside the repository')
            matched = [str(p.relative_to(root)) for p in root.glob(pattern) if p.is_file()]
            if not matched:
                raise ValueError(f'Empty source ownership pattern: {pattern}')
            owned.update(matched)
        hooks = implementations.get(provider.get('adapter'), {})
        for model in provider['models']:
            if 'public' in model and not isinstance(model['public'], bool):
                raise ValueError(f'{model["id"]}: public visibility must be boolean')
            if set(model.get('overrides', {})) - set(provider['features']):
                raise ValueError(f'{model["id"]}: unknown feature override')
            models.append({**model, 'provider': provider['id'], 'brand': model.get('brand', provider['brand']),
                           'family': model.get('family', provider['family']), 'scope': provider['scope']})
            fixture = model.get('fixture')
            if fixture and fixture not in lab['fixtures']:
                raise ValueError(f'Unknown lab fixture: {fixture}')
            if fixture and lab['fixtures'][fixture]['driver'] != provider['id']:
                raise ValueError(f'{model["id"]}: fixture belongs to another driver')
            for feature in scoped:
                declaration = model.get('overrides', {}).get(feature['id'], provider['features'][feature['id']])
                state = declaration['state']
                if state not in STATES or not declaration.get('reason'):
                    raise ValueError(f'{model["id"]}:{feature["id"]}: state and reason required')
                if state != 'unreviewed' and not declaration.get('references'):
                    raise ValueError(f'{model["id"]}:{feature["id"]}: reviewed dispositions need source references')
                references(root, declaration.get('references', []))
                required = feature.get('required_hooks', [])
                missing = [name for name in required if name not in hooks]
                wiring = ('not_applicable' if state == 'not_applicable' else
                          'shared' if provider['scope'] == 'host' or not required else
                          'missing' if len(missing) == len(required) else 'partial' if missing else 'present')
                if state == 'implemented' and missing:
                    raise ValueError(f'{model["id"]}:{feature["id"]}: implemented claim has missing hooks: {missing}')
                paths = sorted(owned | set(feature['shared_paths'] + declaration.get('references', [])))
                proof = evidence_for(root, list(evidence) + provider.get('evidence', []), model['id'], feature, paths)
                required_proof = [kind for kind in feature['required_evidence']
                                  if model.get('physical', True) or kind not in {'hardware', 'emulator'}]
                complete = state == 'implemented' and not missing and all(
                    proof.get(kind, {}).get('status') == 'pass' for kind in required_proof)
                cell = {'id': f'{model["id"]}:{feature["id"]}', 'model': model['id'],
                        'feature': feature['id'], 'provider': provider['id'], 'implementation': state,
                        'reason': declaration['reason'], 'wiring': wiring, 'missing_hooks': missing,
                        'public_note': declaration.get('public_note'),
                        'hook_sources': {name: hooks[name] for name in required if name in hooks},
                        'evidence': proof, 'required_evidence': required_proof, 'complete': complete,
                        'source_paths': paths, 'acceptance': feature['acceptance'],
                        'fixture': fixture, 'lab_status': lab['fixtures'][fixture]['status'] if fixture else 'unavailable',
                        'lab_reason': lab['fixtures'][fixture].get('reason') if fixture else 'No exact-model fixture declared'}
                cells.append(cell)
    for target in targets['models']:
        if target.get('fixture') and lab['fixtures'][target['fixture']]['status'] != 'blocked':
            raise ValueError(f'{target["id"]}: runnable fixture needs a registered integration manifest')
        models.append({**target, 'scope': 'provider', 'provider': None})
        for feature in (f for f in features if f['scope'] == 'provider'):
            cells.append({'id': f'{target["id"]}:{feature["id"]}', 'model': target['id'],
                          'feature': feature['id'], 'provider': None, 'implementation': 'blocked',
                          'reason': target['reason'], 'wiring': 'missing', 'complete': False,
                          'missing_hooks': feature.get('required_hooks', []), 'hook_sources': {},
                          'evidence': {}, 'required_evidence': feature['required_evidence'],
                          'source_paths': target['references'] + feature['shared_paths'],
                          'acceptance': feature['acceptance'], 'fixture': target.get('fixture'),
                          'lab_status': 'blocked', 'lab_reason': target['reason']})
        references(root, target['references'])
    if len({m['id'] for m in models}) != len(models):
        raise ValueError('Duplicate model ID')
    model_ids = {m['id'] for m in models}
    if len(model_ids) != len(models):
        raise ValueError('Duplicate model ID')
    identities = {alias for model in models for alias in model.get('identities', [model['id']])}
    for provider in manifests:
        for guard in provider.get('identity_guards', []):
            references(root, [guard['path']])
            source = (root / guard['path']).read_text()
            blocks = list(regions(source, r'\bfn\s+' + re.escape(guard['function']) + r'\b[^\{]*\{'))
            if len(blocks) != 1:
                raise ValueError(f'Model identity guard changed: {guard}')
            _, start, end = blocks[0]
            tokens = set(re.findall(r'"([a-z0-9-]{1,32})"', source[start:end]))
            unknown = tokens - identities - set(guard.get('ignore', []))
            if unknown:
                raise ValueError(f'{provider["id"]}: model identities need catalogue rows: {sorted(unknown)}')
    for record in list(evidence) + [r for p in manifests for r in p.get('evidence', [])]:
        if record.get('model') not in model_ids or record.get('feature') not in feature_ids:
            raise ValueError('Evidence names an unknown model or feature')
    fixtures = {m.get('fixture') for m in models}
    if set(lab['fixtures']) - fixtures:
        raise ValueError(f'Lab models missing from catalogue: {sorted(set(lab["fixtures"])-fixtures)}')
    return {'schema': 1, 'models': models, 'features': features, 'cells': cells,
            'notice': 'Hook presence is wiring evidence only. No hardware or UI support is inferred.'}


def brief(report, cell):
    feature = next(f for f in report['features'] if f['id'] == cell['feature'])
    model = next(m for m in report['models'] if m['id'] == cell['model'])
    lines = [f'Task {cell["id"]}: {model["brand"]} {model["name"]} / {feature["label"]}',
             f'Implementation: {cell["implementation"]}. Wiring: {cell["wiring"]}.', cell['reason'],
             'Read AGENTS.md and scripts/mixer-lab/README.md first.',
             'Work in provider-owned modules and existing neutral contracts. Do not add branded branches to shared flows.',
             'Do not assume counts, ranges, routing, laws, or option sets from another model.',
             'Do not expand desk protocol tests or add frontend test suites.', 'Acceptance:']
    lines += [f'- {key}: {value}' for key, value in cell['acceptance'].items()]
    lines += ['Source ownership:'] + [f'- {p}' for p in cell['source_paths']]
    lines += [f'Missing hooks: {", ".join(cell["missing_hooks"]) or "none detected (behaviour still needs review)"}',
              f'Exact-model lab: {cell["fixture"] or "none"} ({cell["lab_status"]}); {cell["lab_reason"] or "emulator proof only"}']
    if cell['lab_status'] == 'runnable':
        lines.append(f'Lab: python3 scripts/mixer-lab/lab.py run {cell["fixture"]}')
    lines += ['Stop lab emulators before repository gates.', 'Validation: cargo test --workspace --locked; flutter analyze in app.',
              'For UI changes verify manually in the running app (the Linux desktop app qualifies) and record that platform; record unavailable hardware or platforms as blocked.',
              'Completion requires explicit per-model evidence: ' + ', '.join(cell['required_evidence']),
              'Reuse shared provider implementation where appropriate, but verify each model separately; never mark sibling models verified by inheritance.',
              'Update the owning integration.json disposition and attach checksummed, source-bound evidence.',
              f'Check: python3 scripts/mixer-lab/integration_matrix.py check --complete {cell["id"]}',
              'Leave missing evidence open. Do not turn an emulator pass into a physical support claim.']
    return '\n'.join(lines) + '\n'


def markdown(report):
    lines = ['# OpenFader integration matrix', '', report['notice'], '',
             'Cells show implementation / wiring. Complete requires current evidence for the exact model.', '']
    for scope in ['provider', 'host']:
        features = [f for f in report['features'] if f['scope'] == scope]
        lines += [f'## {scope.title()} features', '', '| Brand / model | ' + ' | '.join(f['label'] for f in features) + ' |',
                  '| --- | ' + ' | '.join('---' for _ in features) + ' |']
        indexed = {(c['model'], c['feature']): c for c in report['cells']}
        for model in report['models']:
            if model['scope'] == scope:
                values = [indexed[model['id'], f['id']] for f in features]
                lines.append(f'| {model["brand"]} {model["name"]} | ' + ' | '.join(
                    'complete' if c['complete'] else f'{c["implementation"]} / {c["wiring"]}' for c in values) + ' |')
        lines.append('')
    return '\n'.join(lines)


def public_report(report):
    """Public projection: no filesystem paths, artifacts or internal task briefs."""
    labels = {'unreviewed': 'Details pending', 'missing': 'Not available',
              'partial': 'Partial', 'implemented': 'Implemented',
              'blocked': 'Not available', 'not_applicable': 'Not applicable'}
    models = [m for m in report['models'] if m['scope'] == 'provider' and m.get('public', True)]
    model_ids = {m['id'] for m in models}
    return {
        'schema': 1,
        'models': [{**{key: m[key] for key in ('id', 'brand', 'name', 'family', 'scope')},
                    'status': m.get('public_status', 'Unsupported' if m.get('model_state') == 'explicitly rejected' else 'Planned')}
                   for m in models],
        'features': [{'id': f['id'], 'label': f.get('public_label', f['label']), 'scope': f['scope']}
                     for f in sorted(report['features'], key=lambda f: f.get('public_order', 100))
                     if f['scope'] == 'provider'],
        'cells': [{'id': c['id'], 'model': c['model'], 'feature': c['feature'],
                   'state': 'verified' if c['complete'] else c['implementation'],
                   'label': 'Verified' if c['complete'] else labels[c['implementation']],
                   'note': c.get('public_note')}
                  for c in report['cells'] if c['model'] in model_ids],
    }


def html_report(report):
    payload = {**report, 'briefs': {c['id']: brief(report, c) for c in report['cells']}}
    data = json.dumps(payload).replace('<', '\\u003c')
    return '''<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>OpenFader integration matrix</title><style>
:root{color-scheme:dark;font:14px system-ui;background:#131715;color:#e9eeea}body{margin:24px}h1{font-size:26px;margin:0 0 8px}p{color:#b7c3bb;max-width:1050px;line-height:1.5}header{margin-bottom:20px}select,input,button{font:inherit;color:inherit;background:#243028;border:1px solid #4c6052;border-radius:6px;padding:9px;margin:3px}button{cursor:pointer}#table{overflow:auto;max-height:68vh;border:1px solid #394a3e}table{border-collapse:separate;border-spacing:0;min-width:100%}th,td{padding:10px;border-bottom:1px solid #344139;border-right:1px solid #344139;text-align:left;min-width:125px}th{position:sticky;top:0;background:#243028;z-index:2;font-weight:500}td:first-child,th:first-child{position:sticky;left:0;background:#1b241e;min-width:210px;z-index:1}th:first-child{z-index:3}td button{width:100%;margin:0;text-align:left;background:transparent;border:0;padding:0}.missing{color:#ffae9d}.blocked{color:#f6d48d}.partial{color:#f4ca85}.present{color:#afcfbc}.complete{color:#c5f17d}small{display:block;color:#a7b4ab;margin-top:5px;font-size:11px}dialog{color:inherit;background:#19221c;border:1px solid #647a69;border-radius:10px;width:min(800px,85vw)}dialog::backdrop{background:#000a}pre{white-space:pre-wrap;font:13px/1.5 ui-monospace,monospace;max-height:60vh;overflow:auto}.legend{font-size:12px}#summary{padding:10px 0;color:#d6e1d8}</style>
<header><h1>OpenFader integration matrix</h1><p>Control your mixer from any device</p><p>Track module wiring, implementation review, and exact-model evidence separately. A present hook can still be a stub, reject a model, or lack UI integration. Click a cell for ownership, acceptance criteria and an agent task.</p></header>
<label>Scope <select id="scope"><option value="provider">Mixer models</option><option value="host">Recorder host (independent of mixer)</option></select></label>
<label>Brand <select id="brand"><option value="">All brands</option></select></label>
<label>Feature <select id="feature"><option value="">All features</option></select></label>
<input id="search" placeholder="Search brand, family or model" aria-label="Search models">
<label><input type="checkbox" id="gaps">Only open work</label>
<div id="summary"></div><p class="legend">Wiring: present / partial / missing / shared. Implementation: unreviewed / missing / partial / implemented / blocked / not applicable. Complete requires current checksummed evidence. Emulator availability is not an emulator pass.</p>
<div id="table"></div><dialog id="detail"><h2 id="title"></h2><pre id="body"></pre><p id="proof"></p><button id="copy">Copy agent task</button><button id="close">Close</button></dialog>
<script id="data" type="application/json">''' + data + '''</script><script>
const d=JSON.parse(document.getElementById('data').textContent),$=id=>document.getElementById(id), esc=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
for(const b of [...new Set(d.models.map(m=>m.brand))].sort())$('brand').add(new Option(b,b));
for(const f of d.features)$('feature').add(new Option(f.label,f.id));
const cells=new Map(d.cells.map(c=>[c.id,c]));let selected='';
function draw(){const scope=$('scope').value, fs=d.features.filter(f=>f.scope===scope&&(!$('feature').value||f.id===$('feature').value));const ms=d.models.filter(m=>m.scope===scope&&(!$('brand').value||m.brand===$('brand').value)&&[m.brand,m.family,m.name].join(' ').toLowerCase().includes($('search').value.toLowerCase())&&(!$('gaps').checked||fs.some(f=>{const c=cells.get(m.id+':'+f.id);return !c.complete&&c.implementation!=='not_applicable'})));const shown=ms.flatMap(m=>fs.map(f=>cells.get(m.id+':'+f.id)));$('summary').textContent=ms.length+' models · '+shown.filter(c=>c.complete).length+' complete · '+shown.filter(c=>c.wiring==='missing').length+' missing wiring · '+shown.filter(c=>c.implementation==='blocked').length+' blocked · '+shown.filter(c=>c.implementation==='unreviewed').length+' unreviewed';$('table').innerHTML='<table><thead><tr><th>Brand / family / model</th>'+fs.map(f=>'<th>'+esc(f.label)+'</th>').join('')+'</tr></thead><tbody>'+ms.map(m=>'<tr><td>'+esc(m.brand+' '+m.name)+'<small>'+esc(m.family)+' · '+esc(m.model_state)+'</small></td>'+fs.map(f=>{const c=cells.get(m.id+':'+f.id);return '<td><button class="'+(c.complete?'complete':c.implementation==='blocked'?'blocked':c.wiring)+'" data-cell="'+esc(c.id)+'">'+esc(c.complete?'Complete':c.implementation.replaceAll('_',' '))+'<small>'+esc(c.wiring)+' wiring · '+Object.values(c.evidence).filter(p=>p.status==='pass').length+'/'+c.required_evidence.length+' evidence types</small></button></td>'}).join('')+'</tr>').join('')+'</tbody></table>';document.querySelectorAll('[data-cell]').forEach(b=>b.onclick=()=>openCell(b.dataset.cell));}
function openCell(id){selected=id;const c=cells.get(id);$('title').textContent=id;$('body').textContent=d.briefs[id];$('proof').textContent='Evidence: '+(Object.entries(c.evidence).map(([k,v])=>k+': '+v.status+' ('+v.artifact+')').join('; ')||'None attached. Implementation and wiring do not prove support.');$('detail').showModal();}
for(const id of ['scope','brand','feature','search','gaps'])$(id).addEventListener('input',draw);$('close').onclick=()=>$('detail').close();$('copy').onclick=async()=>{try{await navigator.clipboard.writeText(d.briefs[selected]);$('copy').textContent='Copied'}catch(e){$('body').focus();const r=document.createRange();r.selectNodeContents($('body'));getSelection().removeAllRanges();getSelection().addRange(r);$('copy').textContent='Task selected; copy with keyboard'}};draw();
</script>'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', action='append', default=[], help='JSON list of granular source-bound evidence records')
    sub = parser.add_subparsers(dest='command', required=True)
    check = sub.add_parser('check', help='Check catalogue integrity, not feature completion')
    check.add_argument('--complete', action='append', default=[], help='Require an exact model:feature task to be complete')
    report = sub.add_parser('report')
    report.add_argument('--format', choices=['json', 'public-json', 'markdown', 'html'], default='markdown')
    report.add_argument('--output', type=Path)
    queue = sub.add_parser('tasks')
    queue.add_argument('--model')
    queue.add_argument('--feature')
    queue.add_argument('--provider')
    task = sub.add_parser('task')
    task.add_argument('id')
    args = parser.parse_args()
    try:
        records = [record for file in args.evidence for record in load(Path(file))]
        data = build(evidence=records)
        indexed = {c['id']: c for c in data['cells']}
        if args.command == 'check':
            for key in args.complete:
                if key not in indexed or not indexed[key]['complete']:
                    raise ValueError(f'{key}: not complete; inspect task and attach current exact-model evidence')
            print(f'Catalogue consistent: {len(data["models"])} models, {len(data["features"])} features, {len(data["cells"])} cells. This is not a support certification.')
        elif args.command == 'task':
            if args.id not in indexed:
                raise ValueError(f'Unknown task {args.id}')
            print(brief(data, indexed[args.id]), end='')
        elif args.command == 'tasks':
            selected = [c for c in data['cells'] if not c['complete'] and c['implementation'] != 'not_applicable'
                        and (not args.model or c['model'] == args.model) and (not args.feature or c['feature'] == args.feature)]
            if args.provider:
                selected = [c for c in selected if c['provider'] == args.provider]
            for cell in sorted(selected, key=lambda c: (c['wiring'] not in {'missing', 'partial'}, c['id'])):
                print(f'{cell["id"]}\t{cell["implementation"]}\t{cell["wiring"]}\t{cell["reason"]}')
        else:
            content = (json.dumps(public_report(data) if args.format == 'public-json' else data, indent=2)
                       if args.format in {'json', 'public-json'} else html_report(data) if args.format == 'html' else markdown(data))
            if args.output:
                args.output.write_text(content)
                print(args.output.resolve())
            else:
                print(content)
        return 0
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(f'Integration matrix: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
