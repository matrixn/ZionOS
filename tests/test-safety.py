"""Exercise the real header reader and overlay generator without block devices."""
import os
from pathlib import Path
import subprocess
import socket
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class HeaderSafety(unittest.TestCase):
    def probe(self, content, *, missing=False, broken_reader=False):
        source = (ROOT / "platform/alpine/zion-lab-provision").read_text()
        # Source only the declarations before validate/main; never run --apply.
        declarations = source.split("\nvalidate() {", 1)[0]
        self.assertIn("check_blank_header() {", declarations)
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory) / "header"
            if not missing:
                fixture.write_bytes(content)
            environment = os.environ.copy()
            if broken_reader:
                reader = Path(directory) / "dd"
                # Even a failed reader that produced a full zero buffer must fail.
                reader.write_text('#!/bin/sh\n/bin/dd "$@"\nexit 1\n')
                reader.chmod(0o755)
                environment["PATH"] = directory + os.pathsep + environment["PATH"]
            return subprocess.run(
                ["sh", "-c", declarations + '\nTARGET="$1"\ncheck_blank_header',
                 "header-test", str(fixture)],
                env=environment, capture_output=True, text=True, check=False,
            )

    def test_complete_zero_header_is_accepted(self):
        self.assertEqual(self.probe(bytes(4096)).returncode, 0)

    def test_short_and_empty_headers_are_denied(self):
        for length in (0, 1, 4095):
            with self.subTest(length=length):
                self.assertNotEqual(self.probe(bytes(length)).returncode, 0)

    def test_nonzero_bytes_anywhere_in_header_are_denied(self):
        for offset in (0, 2048, 4095):
            with self.subTest(offset=offset):
                content = bytearray(4096)
                content[offset] = 1
                self.assertNotEqual(self.probe(content).returncode, 0)

    def test_read_error_is_denied(self):
        self.assertNotEqual(self.probe(b"", missing=True).returncode, 0)
        self.assertNotEqual(self.probe(bytes(4096), broken_reader=True).returncode, 0)


class OverlaySafety(unittest.TestCase):
    def test_archive_boot_chain_and_assets(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            stage = directory / "stage"
            stage.mkdir()
            for name in ("zion-core", "index.html", "app.js", "zion-core.initd"):
                (stage / name).write_text("fixture\n")
            # The production builder mounts this repository at /workspace.
            # Relocate that single literal for an unprivileged local fixture.
            script = (ROOT / "platform/alpine/genapkovl-zionos.sh").read_text()
            script = script.replace("/workspace/", str(ROOT) + "/")
            result = subprocess.run(
                ["sh", "-c", script, "overlay-test", "zionos"], cwd=directory,
                env={**os.environ, "ZION_STAGE": str(stage)},
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            with tarfile.open(directory / "zionos.apkovl.tar.gz") as archive:
                inittab = archive.extractfile("etc/inittab").read().decode()
                self.assertIn("::sysinit:/usr/local/sbin/zion-init-wrapper", inittab)
                self.assertIn("::sysinit:/sbin/openrc boot", inittab)
                self.assertIn("::wait:/sbin/openrc default", inittab)
                self.assertIn("::shutdown:/sbin/openrc shutdown", inittab)
                wrapper = archive.getmember("usr/local/sbin/zion-init-wrapper")
                self.assertTrue(wrapper.mode & 0o111)
                text = archive.extractfile(wrapper).read().decode()
                self.assertIn("exec /sbin/openrc sysinit", text)
                self.assertIn("ZION OS", text)
                self.assertNotIn("sbin/zion-init-wrapper", archive.getnames())
                for name in ("usr/local/sbin/zion-lab-provision",
                             "usr/local/sbin/zion-lab-recover",
                             "etc/init.d/zion-core", "opt/zionos/bin/zion-core"):
                    self.assertTrue(archive.getmember(name).mode & 0o111, name)
                for name in ("etc/runlevels/default/zion-core",
                             "etc/runlevels/boot/networking",
                             "etc/runlevels/sysinit/devfs",
                             "etc/runlevels/shutdown/killprocs"):
                    self.assertTrue(archive.getmember(name).issym(), name)
                for name in ("opt/zionos/web/index.html", "opt/zionos/web/app.js"):
                    self.assertGreater(archive.getmember(name).size, 0)
            (stage / "app.js").write_text("")
            rejected = subprocess.run(
                ["sh", "-c", script, "overlay-test", "invalid"], cwd=directory,
                env={**os.environ, "ZION_STAGE": str(stage)},
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(rejected.returncode, 0)
            self.assertFalse((directory / "invalid.apkovl.tar.gz").exists())


class InitramfsSafety(unittest.TestCase):
    def test_banner_precedes_upstream_body_and_runs_without_external_commands(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            upstream = directory / "alpine-init"
            body = "# original Alpine logic\nprintf 'UPSTREAM_INIT_STARTED\\n'\n"
            upstream.write_text("#!/bin/sh\n" + body)
            output = directory / "init"
            command = ["sh", str(ROOT / "scripts/brand-initramfs.sh"), str(upstream),
                       str(ROOT / "platform/alpine/initramfs-banner.sh"), str(output)]
            subprocess.run(command, check=True, capture_output=True)
            branded = output.read_text()
            self.assertTrue(branded.endswith(body))
            self.assertEqual(branded.count("ZIONOS_INITRAMFS_BANNER"), 1)
            self.assertTrue(output.stat().st_mode & 0o111)
            result = subprocess.run(
                ["/bin/sh", str(output)], env={"PATH": "/nonexistent"},
                capture_output=True, text=True, check=True,
            )
            self.assertLess(result.stdout.index("[INITRAMFS]"),
                            result.stdout.index("UPSTREAM_INIT_STARTED"))
            # An incompatible upstream entry point or duplicate injection must fail.
            upstream.write_text("#!/bin/bash\n" + body)
            self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)
            command[2] = str(output)
            self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)


class VmSmokeSafety(unittest.TestCase):
    def test_occupied_port_is_rejected_before_launch(self):
        with socket.socket() as listener:
            try:
                listener.bind(("127.0.0.1", 8080))
                listener.listen()
            except OSError:
                self.skipTest("Port 8080 already belongs to another process")
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/test-vm.sh")],
                capture_output=True, text=True, timeout=10, check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Port 8080 is unavailable", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
