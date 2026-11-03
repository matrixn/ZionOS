"""Host safety regression tests: mocked tools, regular temporary files only."""
from pathlib import Path
import importlib.util
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('images', Path(__file__).with_name('images.py'))
images = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(images)


class ImageSafety(unittest.TestCase):
    def test_outside_build_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(images.subprocess, 'run') as run:
            with self.assertRaises(ValueError):
                images.create_image(tmp)
            run.assert_not_called()

    def test_existing_and_dangling_files_are_never_overwritten(self):
        with tempfile.TemporaryDirectory(dir=images.ROOT / 'build') as tmp:
            folder = Path(tmp)
            for name in ('filesystem.img', 'persistent.qcow2'):
                target = folder / name
                target.write_text('user image')
                with patch.object(images.subprocess, 'run') as run:
                    with self.assertRaises(FileExistsError):
                        images.create_image(folder)
                    run.assert_not_called()
                self.assertEqual(target.read_text(), 'user image')
                target.unlink()
                target.symlink_to(folder / 'absent')
                with self.assertRaises(FileExistsError):
                    images.create_image(folder)
                target.unlink()

    def test_symlink_workspace_is_refused(self):
        with tempfile.TemporaryDirectory(dir=images.ROOT / 'build') as tmp:
            folder = Path(tmp)
            (folder / 'real').mkdir()
            (folder / 'link').symlink_to(folder / 'real', target_is_directory=True)
            with self.assertRaises(ValueError):
                images.create_image(folder / 'link')

    def test_tools_only_receive_new_pinned_regular_files(self):
        import os
        import stat
        calls = []
        def tool(args, **kwargs):
            self.assertTrue(kwargs['check'])
            self.assertTrue(kwargs['pass_fds'])
            for fd in kwargs['pass_fds']:
                self.assertTrue(stat.S_ISREG(os.fstat(fd).st_mode))
                self.assertEqual(os.fstat(fd).st_nlink, 1)
            for arg in args:
                self.assertFalse(str(arg).startswith('/dev/'))
            calls.append(args)
        with tempfile.TemporaryDirectory(dir=images.ROOT / 'build') as tmp:
            with patch.object(images.subprocess, 'run', side_effect=tool):
                disk = images.create_image(tmp)
            self.assertTrue(disk.is_file())
            self.assertFalse((Path(tmp) / 'filesystem.img').exists())
            self.assertEqual([args[0] for args in calls], ['mkfs.ext4', 'qemu-img'])


if __name__ == '__main__':
    unittest.main()
