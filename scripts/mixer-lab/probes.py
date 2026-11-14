"""Manual probe selection for the registered lab fixtures."""
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCENARIOS = {
    'x32': ('processing', 'fx', 'routing', 'reconnect', 'native', 'console', 'input-processors'),
    'wing': ('processing', 'fx', 'source', 'sends', 'lifecycle', 'processors', 'console'),
}


def stages(fixture, quick, scenario):
    driver = fixture['driver']
    identity = ['--expected-model', fixture['expected_model']]
    full = HERE / f'verify-{driver}-full.dart'
    timeout = 1200 if driver == 'wing' else 900
    if scenario:
        if scenario == 'console':
            return [('console', HERE / 'verify-console-controls.dart', ['--driver', driver], 900)]
        if scenario == 'input-processors':
            return [(scenario, HERE / 'verify-input-processors.dart', [], 900)]
        if scenario == 'native':
            return [(scenario, HERE / 'verify-x32-native.dart', [], 600)]
        return [(scenario, full, ['--scenario', scenario] +
                 (identity if driver == 'wing' else []), timeout)]
    selected = [('basics', HERE / 'verify-basics.dart',
                 ['--driver', driver] + identity, 180)]
    if driver == 'x32':
        selected.append(('extended', HERE.parent / 'verify-x32-emulator.dart',
                         ['--emulator', '127.0.0.1'], 240))
    if not quick:
        selected.append(('full', full, identity if driver == 'wing' else [], timeout))
        selected.append(('console', HERE / 'verify-console-controls.dart', ['--driver', driver], 900))
        if driver == 'x32':
            selected.append(('native', HERE / 'verify-x32-native.dart', [], 600))
            selected.append(('input-processors', HERE / 'verify-input-processors.dart', [], 900))
    return selected
