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
import importlib.util
from unittest import mock
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class LabCliSafety(unittest.TestCase):
    def test_apply_is_denied_before_any_helper(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            helper = directory / "core"
            helper.write_text('#!/bin/sh\necho called > "' + str(directory / "called") + '"\n')
            helper.chmod(0o755)
            source = (ROOT / "platform/alpine/zion-lab-provision").read_text().replace(
                "/opt/zionos/bin/zion-core", str(helper))
            result = subprocess.run(["sh", "-c", source, "cli-test", "--apply"],
                                    capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 1)
            self.assertIn("disabled", result.stderr)
            self.assertFalse((directory / "called").exists())
            self.assertNotIn("mkfs", source)

    def test_plan_validate_and_recovery_use_shared_read_only_core(self):
        with tempfile.TemporaryDirectory() as directory:
            helper = Path(directory) / "core"
            helper.write_text('#!/bin/sh\nprintf "%s\\n" "$1"\n')
            helper.chmod(0o755)
            for script, argument, expected in [
                ("zion-lab-provision", "--plan", "--lab-plan"),
                ("zion-lab-provision", "--validate", "--lab-plan"),
                ("zion-lab-recover", "--check", "--lab-check"),
            ]:
                source = (ROOT / "platform/alpine" / script).read_text().replace(
                    "/opt/zionos/bin/zion-core", str(helper))
                result = subprocess.run(["sh", "-c", source, "cli-test", argument],
                                        capture_output=True, text=True, check=False)
                self.assertEqual(result.returncode, 0)
                self.assertEqual(result.stdout.strip(), expected)

class OverlaySafety(unittest.TestCase):
    def test_archive_boot_chain_and_assets(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            stage = directory / "stage"
            stage.mkdir()
            for name in ("zion-core", "index.html", "app.js", "zion-core.initd", "zion-platform", "zpk", "zpk-runtime"):
                (stage / name).write_text("fixture\n")
            (stage / "apps").mkdir()
            for name in ("registry.js", "bridge.js", "package-center.js", "package-center.css"):
                (stage / "apps" / name).write_text("fixture\n")
            (stage / "catalog").mkdir()
            (stage / "catalog" / "zion-hello-1.0.0.zpk").write_text("fixture\n")
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
                for name in ("opt/zionos/web/index.html", "opt/zionos/web/app.js", "opt/zionos/web/apps/registry.js", "opt/zionos/web/apps/bridge.js"):
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
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
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


class HostImageSafety(unittest.TestCase):
    @staticmethod
    def checker():
        spec=importlib.util.spec_from_file_location("qcow_check",ROOT / "scripts/check-qcow2.py")
        module=importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_backing_and_external_data_headers_are_denied_before_qemu_img(self):
        module=self.checker()
        good=bytearray(104)
        good[:4]=b"QFI\xfb"
        struct.pack_into(">I",good,4,3)
        module.check_header(good)
        cases=[]
        backing=bytearray(good); struct.pack_into(">Q",backing,8,104); cases.append(backing)
        external=bytearray(good); struct.pack_into(">Q",external,72,4); cases.append(external)
        dirty=bytearray(good); struct.pack_into(">Q",dirty,72,1); cases.append(dirty)
        cases.extend([b"", b"QFI\xfb", b"not qcow"])
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"disk.qcow2"
            for data in cases:
                path.write_bytes(data)
                with mock.patch.object(module.subprocess,"check_output",side_effect=AssertionError("must refuse before QEMU opens backing files")):
                    with self.assertRaises(ValueError): module.check(path)
            path.write_bytes(good)
            with mock.patch.object(module.subprocess,"check_output",return_value='{"format":"qcow2","virtual-size":1073741824}'):
                module.check(path)
            link=Path(directory)/"link.qcow2"; link.symlink_to(path)
            with self.assertRaises(ValueError): module.check(link)

    def test_launcher_rejects_drive_network_and_firmware_overrides(self):
        for arguments in (["-drive","file=/dev/sda"],["-nic","bridge"],["-fw_cfg","name=opt/zionos/network,string=bad"]):
            result=subprocess.run(["bash",str(ROOT/"scripts/run-qemu.sh"),*arguments],capture_output=True,text=True,timeout=5)
            self.assertEqual(result.returncode,2)
            self.assertIn("forbidden",result.stderr)


class BuildCacheSafety(unittest.TestCase):
    def test_iso_publication_preserves_open_image_and_failed_build(self):
        spec=importlib.util.spec_from_file_location("publish_iso",ROOT/"scripts/publish-iso.py")
        module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); destination=root/"live.iso"; source=root/"new.iso"
            destination.write_bytes(b"old image"); source.write_bytes(b"new image")
            with destination.open("rb") as running_vm:
                module.publish(source,destination)
                self.assertEqual(running_vm.read(),b"old image")
                self.assertEqual(destination.read_bytes(),b"new image")
            with self.assertRaises(OSError): module.publish(root/"missing",destination)
            self.assertEqual(destination.read_bytes(),b"new image")
            self.assertEqual(list(root.glob(".iso-*")),[])

    def test_source_and_signing_changes_invalidate_correct_layers(self):
        spec=importlib.util.spec_from_file_location("build_inputs",ROOT/"scripts/build-inputs.py")
        module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            files=["platform/alpine/Dockerfile.builder","platform/alpine/mkimg.zionos.sh",
                   "platform/alpine/initramfs-banner.sh","platform/alpine/boot-ui.sh",
                   "scripts/brand-initramfs.sh","scripts/build-inputs.py",
                   "build/stage/zion-core","build/stage/app.js"]
            for name in files:
                path=root/name; path.parent.mkdir(parents=True,exist_ok=True); path.write_text("initial")
            keys=root/"keys"; keys.mkdir(); public=keys/"builder.rsa.pub"; public.write_text("public fixture")
            initial_kernel=module.kernel(root,keys,"builder-one")
            initial_overlay=module.overlay(root)
            os.utime(root/"build/stage/zion-core",None)
            self.assertEqual(initial_overlay,module.overlay(root))
            (root/"build/stage/zion-core").write_text("changed binary")
            self.assertNotEqual(initial_overlay,module.overlay(root))
            self.assertEqual(initial_kernel,module.kernel(root,keys,"builder-one"))
            (root/"platform/alpine/boot-ui.sh").write_text("changed initramfs")
            self.assertNotEqual(initial_kernel,module.kernel(root,keys,"builder-one"))
            key_before=module.kernel(root,keys,"builder-one"); public.write_text("rotated public fixture")
            self.assertNotEqual(key_before,module.kernel(root,keys,"builder-one"))
            (root/"platform/alpine/boot-ui.sh").unlink()
            with self.assertRaises(OSError): module.kernel(root,keys,"builder-one")


class LaunchContractSafety(unittest.TestCase):
    def test_only_exact_network_contract_and_valid_serial_are_published(self):
        with tempfile.TemporaryDirectory() as directory:
            directory=Path(directory)
            bin_dir=directory/"bin"; bin_dir.mkdir()
            for tool in ["modprobe","chown"]:
                stub=bin_dir/tool; stub.write_text("#!/bin/sh\nexit 0\n"); stub.chmod(0o755)
            vendor=directory/"vendor"; vendor.write_text("QEMU\n")
            fw=directory/"fw"; (fw/"network").mkdir(parents=True); (fw/"lab").mkdir()
            runtime=directory/"runtime"
            script=(ROOT/"platform/alpine/zion-launch-contract").read_text()
            script=script.replace("/run/zionos",str(runtime)).replace("/sys/class/dmi/id/sys_vendor",str(vendor))
            script=script.replace("/sys/firmware/qemu_fw_cfg/by_name/opt/zionos",str(fw))
            environment={**os.environ,"PATH":str(bin_dir)+os.pathsep+os.environ["PATH"]}
            (fw/"network/raw").write_bytes(b"QEMU-USERNET-LOCALHOST-V1\0")
            (fw/"lab/raw").write_text("zl-0123456789abcdef")
            subprocess.run(["sh","-c",script],env=environment,check=True,capture_output=True)
            self.assertEqual((runtime/"lab-serial").read_text().strip(),"zl-0123456789abcdef")
            self.assertEqual((runtime/"network-contract").stat().st_mode&0o777,0o444)
            (fw/"network/raw").write_text("BRIDGED")
            subprocess.run(["sh","-c",script],env=environment,check=True,capture_output=True)
            self.assertFalse((runtime/"network-contract").exists())
            self.assertFalse((runtime/"lab-serial").exists())
            (fw/"network/raw").write_text("QEMU-USERNET-LOCALHOST-V1")
            (fw/"lab/raw").write_text("zl-not-valid")
            subprocess.run(["sh","-c",script],env=environment,check=True,capture_output=True)
            self.assertFalse((runtime/"lab-serial").exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
