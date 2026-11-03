"""Resume refusals must happen before any QEMU/image tool can run."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('center_vm', Path(__file__).with_name('vm.py'))
vm = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(vm)


class ResumeSafety(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=vm.ROOT / 'build', prefix='center-guard-')
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name)
        self.disk = self.folder / 'persistent.qcow2'
        self.disk.write_bytes(b'owned regular fixture, not a disk device')
        metadata = self.disk.stat()
        self.config = dict(format='ZION_CENTER_V1', uuid='12345678-1234-1234-1234-123456789abc',
                           inode=metadata.st_ino, device=metadata.st_dev)

    def refused(self):
        (self.folder / 'volume.json').write_text(json.dumps(self.config))
        with patch.object(vm.subprocess, 'run') as run, patch.object(vm.subprocess, 'Popen') as popen:
            with self.assertRaises(ValueError):
                vm.boot(self.folder, 'resume-guard', resume=True)
            run.assert_not_called()
            popen.assert_not_called()

    def test_unknown_environment_and_invalid_uuid(self):
        for field, value in [('format', 'FOREIGN'), ('uuid', '../../dev/sda')]:
            with self.subTest(field=field):
                saved = self.config[field]
                self.config[field] = value
                self.refused()
                self.config[field] = saved

    def test_replaced_image_identity(self):
        self.config['inode'] += 1
        self.refused()

    def test_symlink_image(self):
        self.disk.rename(self.folder / 'original')
        self.disk.symlink_to('original')
        self.refused()

    def test_hardlinked_image(self):
        os.link(self.disk, self.folder / 'other-name')
        self.refused()


if __name__ == '__main__':
    unittest.main()
