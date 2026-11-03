"""Three clean QEMU boots of ONE persistent app image, real OpenRC and Chromium."""
from pathlib import Path
import fcntl
import json
import os
import selectors
import signal
import shutil
import shlex
import socket
import subprocess
import sys
import time
import urllib.request
from images import ROOT, workspace, create_image


def registry():
    with urllib.request.urlopen('http://127.0.0.1:8080/api/apps', timeout=10) as reply:
        return json.load(reply)


def acceleration(legacy):
    if legacy:
        return 'tcg', 'Conroe'
    kvm = False
    try:
        with open('/dev/kvm', 'r+b', buffering=0) as device:
            vm = fcntl.ioctl(device.fileno(), 0xae01, 0)
            os.close(vm)
            kvm = True
    except OSError:
        pass
    accel = os.environ.get('ZION_E2E_ACCEL', 'kvm' if kvm else 'tcg')
    if accel not in ('kvm', 'tcg'):
        raise ValueError('ZION_E2E_ACCEL must be kvm or tcg')
    return accel, 'host' if accel == 'kvm' else 'qemu64'


def boot(folder, disk, phase, legacy=False, interactive=False):
    # Reject a busy listener rather than connecting to somebody else's guest.
    with socket.socket() as probe:
        probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        probe.bind(('127.0.0.1', 8080))
    accel, cpu = acceleration(legacy)
    command = ['qemu-system-x86_64', '-accel', accel, '-cpu', cpu, '-m', '2048', '-smp', '2',
               '-cdrom', str(Path(subprocess.check_output(["sh",str(ROOT/"scripts/iso-path.sh")],text=True).strip())),
               '-drive', f'file={folder}/fixtures.iso,media=cdrom,readonly=on,index=3',
               '-drive', f'file={disk},if=none,id=e2e-disk,format=qcow2,cache=none',
               '-device', 'virtio-blk-pci,drive=e2e-disk,serial=ZPKE2ETEST01',
               '-nic', 'user,model=e1000,hostfwd=tcp:127.0.0.1:8080-:8080',
               '-fw_cfg', 'name=opt/zionos/network,string=QEMU-USERNET-LOCALHOST-V1',
               '-boot', 'd', '-serial', 'mon:stdio', '-display', 'none', '-no-reboot']
    assert '-snapshot' not in command
    (folder / f'qemu-{phase}.json').write_text(json.dumps(command, indent=2) + '\n')
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    log = bytearray()
    live_log = (folder / f'vm-{phase}.log').open('wb')
    stage = 'login'
    watcher = None
    watcher_log = bytearray()
    deadline = time.monotonic() + (1200 if legacy else 600)

    def send(text):
        process.stdin.write(text.encode() + b'\n')
        process.stdin.flush()

    def action(name):
        send(f'/mnt/zion-e2e-fixtures/guest-check {name}; echo ZION_E2E_ACTION_DONE_{name}')

    def completed(name):
        marker = f'\nZION_E2E_ACTION_DONE_{name}\r\n'.encode()
        if marker not in log:
            return False
        if f'\nZION_E2E_PASS_{name}\r\n'.encode() not in log:
            raise RuntimeError(f'guest action failed: {name}; inspect {folder}/vm-{phase}.log')
        return True

    def browser(version, removed=False):
        subprocess.run(['node', str(ROOT / 'tests/apps/browser.cjs'), *(['--removed'] if removed else [])],
                       cwd=ROOT, check=True, env={**os.environ, 'ZION_SDK_ORIGIN': 'http://127.0.0.1:8080',
                                                 'ZION_SDK_VERSION': version})

    def watch_start(before, after):
        nonlocal watcher
        watcher = subprocess.Popen(['node', str(ROOT / 'tests/e2e/desktop.cjs'), before, after],
                                   cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        with selectors.DefaultSelector() as ready:
            ready.register(watcher.stdout, selectors.EVENT_READ)
            until = time.monotonic() + 60
            while time.monotonic() < until:
                for key, _ in ready.select(.2):
                    line = key.fileobj.readline()
                    watcher_log.extend(line)
                    if line.strip() == b'ZION_E2E_BROWSER_READY':
                        return
                if watcher.poll() is not None:
                    raise RuntimeError('live desktop observer failed to start')
        raise TimeoutError('live desktop observer readiness')

    def watch_finish():
        nonlocal watcher
        output, _ = watcher.communicate(timeout=50)
        watcher_log.extend(output)
        if watcher.returncode != 0:
            raise RuntimeError('live desktop revocation failed; see desktop log')
        print(output.decode(), end='', flush=True)
        watcher = None

    try:
        while time.monotonic() < deadline:
            for key, _ in selector.select(.2):
                data = key.fileobj.read1(65536)
                if not data and stage != 'shutdown':
                    raise RuntimeError('QEMU exited early')
                log.extend(data)
                live_log.write(data)
                live_log.flush()
            if b'Kernel panic - not syncing' in log:
                raise RuntimeError('kernel panic')
            if stage == 'login' and b'zionos login:' in log:
                send('root'); stage = 'shell'
            elif stage == 'shell' and b'zionos:~#' in log:
                send('mkdir -p /mnt/zion-e2e-fixtures; mount -r -t iso9660 /dev/sr1 /mnt/zion-e2e-fixtures; sh /mnt/zion-e2e-fixtures/guest.sh; echo ZION_E2E_BOOTSTRAP_DONE')
                stage = 'mounts'
            elif stage == 'mounts' and b'\nZION_E2E_BOOTSTRAP_DONE\r\n' in log:
                if b'\nZION_E2E_MOUNTS_READY\r\n' not in log:
                    raise RuntimeError('persistent fixture mounts refused')
                action(phase); stage = 'ready'
            elif stage == 'ready' and completed(phase):
                browser('1.1.0' if phase == 'third' else '1.0.0')
                # Exercise real storage/metrics/provisioning routes without browser mocks.
                subprocess.run(['bash', 'tests/smoke.sh'], cwd=ROOT, check=True)
                if interactive:
                    print('ZION E2E APP READY: http://127.0.0.1:8080 — open Zion Hello. Ctrl+C shuts down this VM cleanly.', flush=True)
                    print('Local guest commands: status | settings-get | settings-set TEXT | upgrade | failure | uninstall | quit', flush=True)
                    selector.register(sys.stdin, selectors.EVENT_READ)
                    while True:
                        for key, _ in selector.select(.5):
                            if key.fileobj is sys.stdin:
                                line = sys.stdin.readline()
                                if not line or line.strip() == 'quit':
                                    raise KeyboardInterrupt
                                verb, _, value = line.strip().partition(' ')
                                if verb == 'status' and not value:
                                    send('zpk status org.zionos.zion-hello')
                                elif verb in ('settings-get', 'upgrade', 'failure', 'uninstall') and not value:
                                    send(f'/mnt/zion-e2e-fixtures/guest-check {verb}')
                                elif verb == 'settings-set' and value.strip() and len(value.encode()) <= 160 and all(ord(c) >= 32 and ord(c) != 127 for c in value):
                                    send(f'/mnt/zion-e2e-fixtures/guest-check settings-set {shlex.quote(value)}')
                                else:
                                    print('Unsupported command or invalid greeting', flush=True)
                                continue
                            data = key.fileobj.read1(65536)
                            if not data:
                                raise RuntimeError('interactive VM exited')
                            log.extend(data)
                            print(data.decode(errors='replace'), end='', flush=True)
                if phase == 'second':
                    watch_start('1.0.0', '1.1.0')
                    action('upgrade'); stage = 'upgrade'
                elif phase == 'third':
                    watch_start('1.1.0', 'removed')
                    action('uninstall'); stage = 'uninstall'
                else:
                    send('zpk stop org.zionos.zion-hello; sync; poweroff'); stage = 'shutdown'
            elif stage == 'upgrade' and completed('upgrade'):
                watch_finish()
                browser('1.1.0')
                assert len(registry()['apps']) == 1 and registry()['apps'][0]['version'] == '1.1.0'
                action('failure'); stage = 'failure'
            elif stage == 'failure' and completed('failure'):
                browser('1.1.0')  # Failed 1.2 candidate never becomes an active desktop app.
                action('interrupt'); stage = 'interrupt'
            elif stage == 'interrupt' and completed('interrupt'):
                assert registry()['apps'] == []  # No UI while the transaction is incomplete.
                send('sync; poweroff'); stage = 'shutdown'
            elif stage == 'uninstall' and completed('uninstall'):
                watch_finish()
                browser('1.1.0', removed=True)
                send('sync; poweroff'); stage = 'shutdown'
            if stage == 'shutdown':
                # Drain serial output during shutdown; a full pipe must not block QEMU.
                if process.poll() is not None:
                    if process.returncode != 0:
                        raise RuntimeError('unclean QEMU exit')
                    print(f'PASS E2E persistent {phase} boot: {cpu}/{accel}', flush=True)
                    return
            elif process.poll() is not None:
                raise RuntimeError('QEMU stopped unexpectedly')
        raise TimeoutError(f'E2E {phase} at {stage}')
    except KeyboardInterrupt:
        send('zpk stop org.zionos.zion-hello; sync; poweroff')
        end = time.monotonic() + 45
        while process.poll() is None and time.monotonic() < end:
            for key, _ in selector.select(.2):
                log.extend(key.fileobj.read1(65536))
        if process.poll() is None or process.returncode != 0:
            raise RuntimeError('interactive clean shutdown failed')
        print(f'Interactive VM stopped; dedicated test disk retained at {disk}', flush=True)
        if not interactive:
            raise
    finally:
        live_log.close()
        if watcher is not None:
            watcher.terminate()
            try:
                output, _ = watcher.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                watcher.kill(); output, _ = watcher.communicate()
            watcher_log.extend(output)
        (folder / f'desktop-{phase}.log').write_bytes(watcher_log)
        selector.close()
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait()
        (folder / f'vm-{phase}.log').write_bytes(log)


def main():
    def interrupted(_signum, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)
    folder = workspace(sys.argv[1])
    mode = sys.argv[2] if len(sys.argv) > 2 else '--vm'
    if mode not in ('--vm', '--legacy-vm', '--interactive'):
        raise ValueError('invalid VM mode')
    seed = folder / 'media'
    release = Path(os.environ['CARGO_TARGET_DIR']) / 'x86_64-unknown-linux-musl/release'
    for source, name in [(release / 'zpk', 'zpk'), (release / 'zpk-runtime', 'zpk-runtime'),
                         (release / 'zpk-e2e-guest', 'guest-check'), (ROOT / 'tests/e2e/guest.sh', 'guest.sh')]:
        destination = seed / name
        if os.path.lexists(destination):
            raise FileExistsError('fixture already exists')
        shutil.copyfile(source, destination)
        destination.chmod(0o755)
    media = folder / 'fixtures.iso'
    if os.path.lexists(media):
        raise FileExistsError('fixture ISO already exists')
    subprocess.run(['xorriso', '-as', 'mkisofs', '-R', '-J', '-o', str(media), str(seed)], check=True)
    disk = create_image(folder)
    print(f'E2E artifacts and dedicated persistent disk: {folder}', flush=True)
    for phase in (('first',) if mode == '--interactive' else ('first', 'second', 'third')):
        boot(folder, disk, phase, legacy=mode == '--legacy-vm', interactive=mode == '--interactive')
    print('PASS E2E lifecycle' if mode != '--interactive' else 'Interactive development session complete', flush=True)


if __name__ == '__main__':
    main()
