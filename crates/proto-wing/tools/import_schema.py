#!/usr/bin/env python3
"""Extract WING FX protocol facts from a local libwing propmap.jsonl.

Usage: python3 tools/import_schema.py /path/to/libwing/propmap.jsonl
Run from any directory. See src/schema/README.md for source revision/license.
Does not access or modify a mixer.
"""
import json
from pathlib import Path
import struct
import sys


def number(value):
    """Shortest decimal preserving the schema's float32 value."""
    bits = struct.pack('>f', value)
    for precision in range(1, 10):
        text = format(value, f'.{precision}g')
        if struct.pack('>f', float(text)) == bits:
            if '.' not in text and 'e' not in text:
                text += '.0'
            return text + 'f32'
    raise ValueError(value)


def quoted(value):
    return json.dumps(str(value), ensure_ascii=False)


def main():
    rows = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines()]
    models = next(r['items'] for r in rows if r['fullname'] == '/fx/1/mdl')
    small = {item['item'] for item in next(r['items'] for r in rows if r['fullname'] == '/fx/9/mdl')}
    kinds = {'linear float': 'Linear', 'log float': 'Log', 'integer': 'Int',
             'string enum': 'Enum', 'float enum': 'FloatEnum', 'fader level': 'Fader'}
    output = ['// Generated factual protocol table. libwing revision 74c2ff6, WING 3.0.5.\n',
              '// Preserve schema/LICENSE.libwing when redistributing.\n',
              '#[rustfmt::skip]\npub const MODELS: &[FxModel] = &[\n']
    for model in models:
        code = model['item']
        prefix = '/fx/1/' + code + '/'
        params = [next(r for r in rows if r['fullname'] == '/fx/1/fxmix')]
        params += [r for r in rows if r['fullname'].startswith(prefix)]
        output.append(f'    FxModel {{ id: {quoted(code)}, label: {quoted(model.get("longitem", code))}, premium: {str(code not in small).lower()}, params: &[\n')
        for row in params:
            kind = row['type']
            items = row.get('items', [])
            low = row.get('minfloat', row.get('minint', -144 if kind == 'fader level' else 0))
            high = row.get('maxfloat', row.get('maxint', 10 if kind == 'fader level' else len(items) - 1))
            # One-step schema properties are constants, not editable controls.
            if kind not in ('string enum', 'float enum') and low == high:
                continue
            choices = ', '.join('(' + quoted(format(item['item'], '.7g') if isinstance(item['item'], (float, int)) else item['item'])
                                + ', ' + quoted(item.get('longitem', item['item'])) + ')' for item in items)
            values = ', '.join(number(item['item']) for item in items) if kind == 'float enum' else ''
            output.append('        FxParameter { name: ' + quoted(row['name']) + ', label: ' + quoted(row.get('longname', row['name']))
                          + ', unit: ' + quoted(row.get('unit', '')) + ', kind: FxKind::' + kinds[kind]
                          + ', min: ' + number(low) + ', max: ' + number(high) + ', steps: ' + str(row.get('steps', 0))
                          + ', choices: &[' + choices + '], values: &[' + values + '] },\n')
        output.append('    ] },\n')
    output.append('];\n')
    target = Path(__file__).resolve().parents[1] / 'src/schema/fx.rs'
    target.write_text(''.join(output))


if __name__ == '__main__':
    main()
