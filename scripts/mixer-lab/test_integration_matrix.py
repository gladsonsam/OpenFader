"""Inventory consistency tests only; no frontend or desk protocol tests."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('integration_matrix', Path(__file__).with_name('integration_matrix.py'))
matrix = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(matrix)


class MatrixTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.put('crates/engine/src/drivers.rs', '''
pub trait DynDriver { fn recorder_descriptor(&self) {} }
impl_dyn_driver!(fixture::Driver {
    async fn recorder_descriptor(&self) { let note = "} fn fake() {"; }
});
fn registry() { vec![RegisteredDriver { name: "fixture", driver: driver() }] }
''')
        self.put('crates/fixture/src/lib.rs', 'pub struct Driver;')
        self.put('app/recorder.dart', '// neutral recorder flow')
        self.feature = {'id': 'recorder_names', 'label': 'Recorder names', 'scope': 'provider',
                        'engine_contracts': ['recorder_descriptor'], 'required_hooks': ['recorder_descriptor'],
                        'shared_paths': ['app/recorder.dart'], 'acceptance': {'patch': 'Read actual patch'},
                        'required_evidence': ['unit', 'hardware', 'ui']}
        self.provider = {'schema': 1, 'id': 'fixture', 'brand': 'Fixture', 'family': 'Fixture',
                         'scope': 'provider', 'adapter': 'fixture::Driver',
                         'source_paths': ['crates/fixture/src/lib.rs'], 'source_globs': ['crates/fixture/src/**/*.rs'],
                         'models': [{'id': 'fixture-one', 'name': 'One', 'model_state': 'fixture'}],
                         'features': {'recorder_names': {'state': 'unreviewed', 'reason': 'Not audited'}}, 'evidence': []}
        self.json('scripts/mixer-lab/integration_features.json', {'schema': 1, 'features': [self.feature]})
        self.json('scripts/mixer-lab/matrix.json', {'schema': 1, 'fixtures': {}})
        self.json('scripts/mixer-lab/integration_targets.json', {'schema': 1, 'models': []})
        self.save_provider()

    def put(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def json(self, path, value):
        self.put(path, json.dumps(value))

    def save_provider(self):
        self.json('crates/fixture/integration.json', self.provider)

    def evidence(self, kind, paths):
        artifact = self.root / f'{kind}.json'
        artifact.write_text(json.dumps({'result': 'pass', 'checks': ['patch']}))
        return {'model': 'fixture-one', 'feature': 'recorder_names', 'kind': kind,
                'result': 'pass', 'checks': ['patch'], 'recorded_at': '2026-01-01T00:00:00Z',
                'firmware': 'fixture-1', 'device': 'fixture simulator',
                'platform': 'fixture desktop', 'artifact': str(artifact),
                'sha256': matrix.digest(artifact), 'source_sha256': {p: matrix.digest(self.root / p) for p in paths}}

    def test_real_catalogue_covers_registered_drivers(self):
        report = matrix.build()
        registered, _, _ = matrix.source_inventory(matrix.ROOT)
        self.assertEqual({m['provider'] for m in report['models'] if m['provider'] and m['scope'] == 'provider'}, registered)
        scopes = {f['id']: f['scope'] for f in report['features']}
        models = {m['id']: m for m in report['models']}
        self.assertTrue(all(scopes[c['feature']] == models[c['model']]['scope'] for c in report['cells']))

    def test_public_projection_contains_no_internal_paths_or_briefs(self):
        report = matrix.build(self.root)
        public = matrix.public_report(report)
        self.assertEqual(public['cells'][0]['label'], 'Details pending')
        self.assertNotIn('source_paths', json.dumps(public))
        self.assertNotIn('artifact', json.dumps(public))
        self.assertNotIn('evidence', json.dumps(public))
        self.assertNotIn('app/recorder.dart', json.dumps(public))
        self.assertNotIn('briefs', public)

    def test_public_compatibility_excludes_host_recording_but_keeps_internal_tracking(self):
        report = matrix.build()
        public = matrix.public_report(report)
        self.assertTrue(any(m['scope'] == 'host' for m in report['models']))
        self.assertTrue(any(f['scope'] == 'host' for f in report['features']))
        self.assertTrue(all(m['scope'] == 'provider' for m in public['models']))
        self.assertTrue(all(f['scope'] == 'provider' for f in public['features']))
        self.assertFalse(any(c['feature'] == 'recording' for c in public['cells']))

    def test_public_models_hide_internal_identities_without_losing_audit_rows(self):
        self.provider['models'][0]['public'] = False
        self.save_provider()
        report = matrix.build(self.root)
        self.assertEqual(len(report['models']), 1)
        self.assertEqual(len(report['cells']), 1)
        self.assertEqual(matrix.public_report(report)['models'], [])
        self.assertEqual(matrix.public_report(report)['cells'], [])

    def test_implemented_public_status_does_not_imply_verification(self):
        self.provider['features']['recorder_names'].update(
            state='implemented', references=['crates/fixture/src/lib.rs'])
        self.save_provider()
        report = matrix.build(self.root)
        self.assertEqual(matrix.public_report(report)['cells'][0]['label'], 'Implemented')
        self.assertFalse(report['cells'][0]['complete'])

    def test_scanner_ignores_comments_strings_nested_functions_and_lifetimes(self):
        code = '''/* outer /* fn false1() {} */ } */
fn actual<'a>(&'a self) { let x = r###" { fn false2() {} "###; fn nested() {} }
// fn false3() {}
async fn second() { let c = '}'; let s = "{ } \\\""; }
'''
        self.assertEqual(set(matrix.methods(code)), {'actual', 'second'})

    def test_new_engine_hook_cannot_be_silently_skipped(self):
        path = self.root / 'crates/engine/src/drivers.rs'
        path.write_text(path.read_text().replace('pub trait DynDriver {', 'pub trait DynDriver { fn new_hook(&self) {}'))
        with self.assertRaisesRegex(ValueError, 'classification'):
            matrix.build(self.root)

    def test_new_driver_requires_manifest(self):
        path = self.root / 'crates/engine/src/drivers.rs'
        path.write_text(path.read_text() + '\nfn another() { RegisteredDriver { name: "forgotten" } }')
        with self.assertRaisesRegex(ValueError, 'mismatch'):
            matrix.build(self.root)

    def test_new_feature_requires_every_provider_disposition(self):
        extra = {**self.feature, 'id': 'new_feature', 'engine_contracts': [], 'required_hooks': []}
        self.json('scripts/mixer-lab/integration_features.json', {'schema': 1, 'features': [self.feature, extra]})
        with self.assertRaisesRegex(ValueError, 'explicit disposition'):
            matrix.build(self.root)

    def test_new_model_identity_requires_catalogue_row(self):
        self.put('crates/fixture/src/identity.rs', 'fn allowed() { matches!(model, "fixture-one" | "forgotten-model") }')
        self.provider['identity_guards'] = [{'path': 'crates/fixture/src/identity.rs', 'function': 'allowed'}]
        self.save_provider()
        with self.assertRaisesRegex(ValueError, 'identities need catalogue rows'):
            matrix.build(self.root)

    def test_hook_presence_is_not_implementation_or_verification(self):
        cell = matrix.build(self.root)['cells'][0]
        self.assertEqual(cell['wiring'], 'present')
        self.assertEqual(cell['implementation'], 'unreviewed')
        self.assertFalse(cell['complete'])

    def test_removed_hook_invalidates_implemented_claim(self):
        self.provider['features']['recorder_names'] = {'state': 'implemented', 'reason': 'Reviewed',
                                                      'references': ['crates/fixture/src/lib.rs']}
        self.save_provider()
        path = self.root / 'crates/engine/src/drivers.rs'
        path.write_text(path.read_text().replace('async fn recorder_descriptor', 'async fn different_name'))
        with self.assertRaisesRegex(ValueError, 'missing hooks'):
            matrix.build(self.root)

    def test_exact_model_evidence_and_source_changes(self):
        self.provider['features']['recorder_names'] = {'state': 'implemented', 'reason': 'Reviewed',
                                                      'references': ['crates/fixture/src/lib.rs']}
        self.save_provider()
        cell = matrix.build(self.root)['cells'][0]
        records = [self.evidence(kind, cell['source_paths']) for kind in cell['required_evidence']]
        self.assertTrue(matrix.build(self.root, records)['cells'][0]['complete'])
        self.assertFalse(matrix.build(self.root, records[:-1])['cells'][0]['complete'])
        self.put('crates/fixture/src/lib.rs', 'pub struct ChangedDriver;')
        changed = matrix.build(self.root, records)['cells'][0]
        self.assertFalse(changed['complete'])
        self.assertEqual({p['status'] for p in changed['evidence'].values()}, {'stale'})

    def test_ui_evidence_records_platform_and_ios_is_optional(self):
        self.provider['features']['recorder_names'] = {'state': 'implemented', 'reason': 'Reviewed',
                                                      'references': ['crates/fixture/src/lib.rs']}
        self.save_provider()
        cell = matrix.build(self.root)['cells'][0]
        records = [self.evidence(kind, cell['source_paths']) for kind in ('unit', 'hardware', 'ui')]
        self.assertTrue(matrix.build(self.root, records)['cells'][0]['complete'])
        ios = self.evidence('ipad', cell['source_paths'])
        self.assertTrue(matrix.build(self.root, [*records, ios])['cells'][0]['complete'])
        ios.pop('device')
        with self.assertRaisesRegex(ValueError, 'device'):
            matrix.build(self.root, [*records, ios])
        records[2].pop('platform')
        with self.assertRaisesRegex(ValueError, 'platform'):
            matrix.build(self.root, records)

    def test_tampered_artifact_is_rejected(self):
        cell = matrix.build(self.root)['cells'][0]
        record = self.evidence('unit', cell['source_paths'])
        Path(record['artifact']).write_text('modified')
        with self.assertRaisesRegex(ValueError, 'checksum'):
            matrix.build(self.root, [record])

    def test_changed_feature_contract_invalidates_old_evidence(self):
        cell = matrix.build(self.root)['cells'][0]
        record = self.evidence('unit', cell['source_paths'])
        self.feature['acceptance']['patch'] = 'Read patch and renamed channels'
        self.json('scripts/mixer-lab/integration_features.json', {'schema': 1, 'features': [self.feature]})
        self.assertEqual(matrix.build(self.root, [record])['cells'][0]['evidence']['unit']['status'], 'stale')

    def test_latest_failure_wins_across_timezone_offsets(self):
        cell = matrix.build(self.root)['cells'][0]
        old = self.evidence('unit', cell['source_paths'])
        old['recorded_at'] = '2026-01-01T12:00:00+12:00'
        newer = {**old, 'recorded_at': '2026-01-01T01:00:00Z', 'result': 'fail'}
        evidence = matrix.build(self.root, [newer, old])['cells'][0]['evidence']
        self.assertEqual(evidence['unit']['status'], 'fail')

    def test_acceptance_omissions_do_not_complete_a_cell(self):
        cell = matrix.build(self.root)['cells'][0]
        record = self.evidence('unit', cell['source_paths'])
        record['checks'] = []
        self.assertEqual(matrix.build(self.root, [record])['cells'][0]['evidence']['unit']['status'], 'stale')

    def test_html_payload_cannot_close_script(self):
        self.provider['features']['recorder_names']['reason'] = '</script><script>bad()</script>'
        self.save_provider()
        page = matrix.html_report(matrix.build(self.root))
        self.assertNotIn('</script><script>bad()', page)
        self.assertIn('\\u003c/script>', page)


if __name__ == '__main__':
    unittest.main()
