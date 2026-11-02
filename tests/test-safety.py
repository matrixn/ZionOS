"""Exercise the real header reader and overlay generator without block devices."""
import os
from pathlib import Path
import subprocess
import socket
import pty
import fcntl
import struct
import select
import time
import re
import signal
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
                self.assertIn("::sysinit:/usr/local/sbin/zion-init-wrapper boot", inittab)
                self.assertIn("::wait:/usr/local/sbin/zion-init-wrapper default", inittab)
                self.assertIn("::shutdown:/sbin/openrc shutdown", inittab)
                wrapper = archive.getmember("usr/local/sbin/zion-init-wrapper")
                self.assertTrue(wrapper.mode & 0o111)
                text = archive.extractfile(wrapper).read().decode()
                self.assertIn('exec /sbin/openrc "$zion_phase"', text)
                self.assertIn("ZION OS", text)
                self.assertGreater(archive.getmember("usr/local/lib/zionos/boot-ui.sh").size, 0)
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
                       str(ROOT / "platform/alpine/initramfs-banner.sh"), str(output),
                       str(ROOT / "platform/alpine/boot-ui.sh")]
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


class BootLoaderSafety(unittest.TestCase):
    def run_wrapper(self, phase="sysinit", status=0, *, terminal=True,
                    cmdline="", size=(24, 80), healthy=True, term="xterm",
                    interrupt=False):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            openrc = directory / "openrc"
            openrc.write_text(
                "#!/bin/sh\nprintf 'FIXTURE_OPENRC_%s\\n' \"$1\"\n"
                "printf 'FIXTURE_UMASK='; umask\n"
                "sleep 0.35\nprintf 'FIXTURE_DIAGNOSTIC\\n' >&2\n"
                f"exit {status}\n"
            )
            openrc.chmod(0o755)
            curl = directory / "curl"
            curl.write_text("#!/bin/sh\n" + (
                "printf '%s' '{\"service\":\"zion-core\",\"status\":\"ok\"}'\n"
                if healthy else "exit 7\n"
            ))
            curl.chmod(0o755)
            (directory / "cmdline").write_text(cmdline)
            source = (ROOT / "platform/alpine/zion-init-wrapper").read_text()
            source = source.replace("/usr/local/lib/zionos/boot-ui.sh",
                                    str(ROOT / "platform/alpine/boot-ui.sh"))
            source = source.replace("/sbin/openrc", str(openrc))
            source = source.replace("/proc/cmdline", str(directory / "cmdline"))
            source = source.replace("/var/log/zion-boot", str(directory / "logs"))
            source = source.replace("curl -fsS", str(curl) + " -fsS")
            wrapper = directory / "wrapper"
            wrapper.write_text(source)
            environment = {**os.environ, "TERM": term}
            if not terminal:
                result = subprocess.run(["sh", str(wrapper), phase], env=environment,
                                        capture_output=True, text=True, timeout=8)
                return result.returncode, result.stdout + result.stderr, {}
            master, slave = pty.openpty()
            fcntl.ioctl(slave, 0x5414, struct.pack("HHHH", *size, 0, 0))
            process = subprocess.Popen(["sh", str(wrapper), phase], env=environment,
                                       stdin=slave, stdout=slave, stderr=slave)
            os.close(slave)
            data = bytearray()
            try:
                deadline = time.monotonic() + 8
                interrupt_at = time.monotonic() + 0.15
                while time.monotonic() < deadline:
                    if interrupt and time.monotonic() >= interrupt_at:
                        process.send_signal(signal.SIGTERM)
                        interrupt = False
                    if select.select([master], [], [], 0.1)[0]:
                        try:
                            block = os.read(master, 65536)
                        except OSError:
                            break
                        if not block:
                            break
                        data.extend(block)
                process.wait(timeout=2)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
                os.close(master)
            logs = {p.name: p.read_text() for p in (directory / "logs").glob("*.log")}
            return process.returncode, data.decode(errors="replace"), logs

    def test_animation_preserves_phase_logs_and_restores_cursor(self):
        inherited_mask = os.umask(0)
        os.umask(inherited_mask)
        status, output, logs = self.run_wrapper()
        self.assertEqual(status, 0)
        self.assertIn("SYSTEM FOUNDATION", output)
        self.assertIn("LIVE BOOT TELEMETRY", output)
        self.assertGreater(output.count("\033[14;1H"), 1)
        self.assertEqual(output.count("SYSTEM FOUNDATION"), 1)
        self.assertIn("\033[19;24r", output)
        self.assertIn("\033[r", output)
        self.assertIn("\033[?25l", output)
        self.assertIn("\033[?25h", output)
        self.assertIn("FIXTURE_OPENRC_sysinit", logs["sysinit.log"])
        self.assertIn("FIXTURE_DIAGNOSTIC", logs["sysinit.log"])
        self.assertIn(f"FIXTURE_UMASK={inherited_mask:04o}", logs["sysinit.log"])

    def test_failed_phase_exposes_diagnostics_and_original_exit_status(self):
        status, output, _ = self.run_wrapper(status=7)
        self.assertEqual(status, 7)
        self.assertIn("[BOOT ERROR]", output)
        self.assertIn("FIXTURE_DIAGNOSTIC", output)
        self.assertNotIn("YOUR NAS IS ONLINE", output)
        self.assertIn("\033[?25h", output)

    def test_ready_requires_successful_api_probe(self):
        status, output, _ = self.run_wrapper(phase="default")
        self.assertEqual(status, 0)
        self.assertIn("YOUR NAS IS ONLINE", output)
        self.assertIn("[BOOT COMPLETE] ready", output)
        status, output, _ = self.run_wrapper(phase="default", healthy=False)
        self.assertEqual(status, 0)
        self.assertNotIn("YOUR NAS IS ONLINE", output)
        self.assertIn("[BOOT COMPLETE] pending", output)

    def test_plain_mode_preserves_boot_for_optout_pipes_and_small_terminals(self):
        for options in ({"terminal": False}, {"cmdline": "zion.splash=off"},
                        {"size": (18, 60)}, {"term": "dumb"}):
            with self.subTest(options=options):
                status, output, _ = self.run_wrapper(status=3, **options)
                self.assertEqual(status, 3)
                self.assertIn("FIXTURE_OPENRC_sysinit", output)
                self.assertNotIn("\033[?25l", output)

    def test_termination_restores_cursor_and_scroll_region(self):
        status, output, _ = self.run_wrapper(interrupt=True)
        self.assertEqual(status, 143)
        self.assertIn("\033[?25l", output)
        self.assertTrue(output.endswith("\033[r\033[0m\033[?25h"))

    def test_frame_fits_an_80_by_24_console_without_external_commands(self):
        source = (ROOT / "platform/alpine/boot-ui.sh").read_text()
        result = subprocess.run(
            ["/bin/sh", "-c", source + "\nzion_boot_frame sysinit 2 12 ''"],
            env={"PATH": "/nonexistent"}, capture_output=True, text=True, check=True,
        )
        plain = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", result.stdout)
        self.assertLessEqual(len(plain.splitlines()), 24)
        self.assertTrue(all(len(line) <= 80 for line in plain.splitlines()))


if __name__ == "__main__":
    unittest.main(verbosity=2)
