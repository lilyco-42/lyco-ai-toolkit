"""Draft retention must preserve exact bytes without publishing or overwriting."""
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('draft_store', Path(__file__).parents[1] / 'scripts/store_ci_draft.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class DraftRetentionTests(unittest.TestCase):
    def test_published_or_wrong_target_release_cannot_be_used(self):
        module.validate_draft({'draft': True, 'target_commitish': 'own'}, 'own')
        for record in ({'draft': False, 'target_commitish': 'own'},
                       {'draft': True, 'target_commitish': 'other'},
                       {'draft': 'true', 'target_commitish': 'own'}):
            with self.assertRaises(ValueError):
                module.validate_draft(record, 'own')

    def test_archive_preserves_files_and_per_file_digests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'out').mkdir()
            (root / 'out/own.txt').write_bytes(b'own fixture')
            data, records = module.archive_files(root, ['out/', 'out/own.txt'])
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                self.assertEqual(['out/own.txt'], archive.namelist())
                self.assertEqual(b'own fixture', archive.read('out/own.txt'))
            self.assertEqual(module.sha(b'own fixture'), records['out/own.txt']['sha256'])

    def test_outside_checkout_and_missing_outputs_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'checkout'
            root.mkdir()
            (root.parent / 'outside.txt').write_bytes(b'own fixture')
            with self.assertRaises(ValueError):
                module.archive_files(root, ['../outside.txt'])
            with self.assertRaises(ValueError):
                module.archive_files(root, ['missing.txt'])

    def test_existing_identical_asset_reused_but_changed_asset_refused(self):
        data = b'own fixture'
        item = {'name': 'own.zip', 'size': len(data), 'digest': 'sha256:' + module.sha(data)}
        self.assertIs(item, module.immutable_upload({'assets': [item]}, 'own.zip', data))
        with self.assertRaises(ValueError):
            module.immutable_upload({'assets': [dict(item, digest='sha256:' + '0' * 64)]}, 'own.zip', data)
