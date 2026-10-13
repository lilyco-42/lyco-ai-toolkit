"""Policy tests only: behavior remains proved by the exact real product workflow."""
import importlib.util
import io
import json
from pathlib import Path
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('reviewed_asset', Path(__file__).parents[1] / 'scripts/reviewed_asset_release.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class FullProductPolicyTests(unittest.TestCase):
    def test_candidates_failed_runs_and_other_repositories_are_rejected(self):
        valid = {'head_sha': module.SOURCE, 'head_branch': 'main', 'event': 'push',
                 'name': 'Licensed Asset Studio', 'status': 'completed', 'conclusion': 'success',
                 'head_repository': {'full_name': module.REPO}}
        module.validate_run(valid)
        for key, value in [('head_sha', '0' * 40), ('head_branch', 'other'), ('event', 'pull_request'),
                           ('status', 'in_progress'), ('conclusion', 'failure'),
                           ('head_repository', {'full_name': 'other/repository'})]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                module.validate_run({**valid, key: value})

    def test_string_boolean_or_missing_installed_evidence_is_rejected(self):
        module.required_flags({'installed': True}, ['installed'])
        for record in ({}, {'installed': 'true'}, {'installed': False}):
            with self.assertRaises(ValueError):
                module.required_flags(record, ['installed'])

    def test_full_manifest_paths_digest_and_runtime_source_are_required(self):
        for bad in ('wrong-source', 'digest', 'unlisted', 'traversal', 'missing-engine', 'missing-license'):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                module.inspect_package(self.package(bad), 'Linux')

    def test_complete_own_fixture_passes_identity_and_distinct_license(self):
        result = module.inspect_package(self.package(), 'Linux')
        self.assertTrue(result['both_original_native_engines_present'])
        self.assertTrue(result['full_spine_notice_verified'])

    def test_exact_windows_text_translation_handles_mixed_upstream_newlines(self):
        record = 'Full own fixture.\nSecond line.\r\nThird line.\n'
        written = record.replace('\n', '\r\n')
        self.assertIn('\r\r\n', written)
        self.assertTrue(module.full_agreement_matches(record, record, 'Linux'))
        self.assertTrue(module.full_agreement_matches(record, written, 'Windows'))
        self.assertFalse(module.full_agreement_matches(record, written, 'Linux'))
        self.assertFalse(module.full_agreement_matches(record, written.replace('Third', 'Changed'), 'Windows'))
        self.assertFalse(module.full_agreement_matches(record, record.replace('\r\n', '\n'), 'Windows'))

    def package(self, bad=None):
        agreement = 'Spine Runtimes License Agreement\n' + 'Own policy fixture.\n' * 80
        records = {'_internal/bin/laic': b'\x7fELF' + b'own-test-fixture' * 100,
                   '_internal/bin/lyco-spine-render': b'\x7fELF' + b'own-test-fixture' * 100,
                   'SPINE-RUNTIME-LICENSE.html': agreement.encode(),
                   'THIRD-PARTY-NOTICES.json': json.dumps([{'ecosystem': 'Spine Runtimes',
                     'license': 'Spine Runtimes License Agreement (not MIT)',
                     'notices': {'current-agreement': agreement}}]).encode(),
                   'UI-package-lock.json': json.dumps({'version': '0.1.1'}).encode()}
        for name in ('LICENSE', 'SPINE-TOOL-SOURCE-LICENSE', 'SPINE-Cargo.lock', 'AIC-Cargo.lock'):
            records[name] = b'Own fixture, never shipped as a license. ' * 10
        if bad == 'platform-newlines':
            records['SPINE-RUNTIME-LICENSE.html'] = agreement.replace('\n', '\r\n').encode()
        if bad == 'missing-engine':
            del records['_internal/bin/lyco-spine-render']
        if bad == 'missing-license':
            records['SPINE-RUNTIME-LICENSE.html'] = b'incomplete'
        manifest = {'commit': module.APP, 'spine_source_commit': module.SOURCE,
                    'spine_runtime_revision': '4ff8b7280aeb7367f235752c30dc85184c01452a',
                    'contains_spine_runtime': True, 'runner': 'Linux',
                    'runtime_license': 'Spine Runtimes License Agreement',
                    'release_id': module.hashlib.sha1((module.APP + '\n' + module.SOURCE).encode()).hexdigest(),
                    'files': {name: module.sha(data) for name, data in records.items()}}
        if bad == 'wrong-source':
            manifest['spine_source_commit'] = '0' * 40
        if bad == 'digest':
            manifest['files']['LICENSE'] = '0' * 64
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, 'w') as archive:
            for name, data in records.items():
                archive.writestr(module.PREFIX + name, data)
            archive.writestr(module.PREFIX + 'manifest.json', json.dumps(manifest))
            if bad == 'unlisted':
                archive.writestr(module.PREFIX + 'unlisted.txt', b'own')
            if bad == 'traversal':
                archive.writestr(module.PREFIX + '../outside.txt', b'own')
        return buffer.getvalue()
