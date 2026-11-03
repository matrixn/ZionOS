"""Native Manager + OpenRC backend + production Core + real browser, on disposable QEMU only."""
from pathlib import Path
import fcntl,importlib.util,os,selectors,shutil,socket,subprocess,tempfile,time
ROOT=Path(__file__).resolve().parents[2]

def main():
    # Policy deliberately requires Host :8080, matching the fixed guest launch contract.
    with socket.socket() as probe:
        try: probe.bind(('127.0.0.1',8080))
        except OSError as error: raise RuntimeError('localhost:8080 is busy; no VM was started') from error
    seed=ROOT/'build/sdk-vm-fixtures';seed.mkdir(parents=True,exist_ok=True)
    release=Path(os.environ['CARGO_TARGET_DIR'])/'x86_64-unknown-linux-musl/release'
    for source,destination in [(release/'zpk','zpk'),(release/'zpk-runtime','zpk-runtime'),(Path(os.environ['ZION_SDK_PACKAGE']),'zion-hello.zpk'),(ROOT/'tests/apps/guest.sh','guest.sh')]:
        if (seed/destination).is_symlink(): raise ValueError('unsafe fixture output')
        shutil.copyfile(source,seed/destination)
        if destination in ('zpk','zpk-runtime'): (seed/destination).chmod(0o755)
    media=ROOT/'build/sdk-vm-fixtures.iso'
    subprocess.run(['xorriso','-as','mkisofs','-R','-J','-o',str(media),str(seed)],check=True)
    spec=importlib.util.spec_from_file_location('manager_vm',ROOT/'tests/zpk/manager-vm.py')
    helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
    kvm=False
    try:
        with open('/dev/kvm','r+b',buffering=0) as device:
            vm=fcntl.ioctl(device.fileno(),0xae01,0);os.close(vm);kvm=True
    except OSError: pass
    accel=os.environ.get('ZION_SDK_ACCEL','kvm' if kvm else 'tcg')
    if accel not in ('kvm','tcg'): raise ValueError('ZION_SDK_ACCEL must be kvm or tcg')
    with tempfile.TemporaryDirectory(prefix='sdk-vm-',dir=ROOT/'build') as temporary:
        disk=helper.create_image(temporary)
        command=['qemu-system-x86_64','-accel',accel,'-cpu','host' if accel=='kvm' else 'qemu64','-m','2048','-smp','2',
                 '-cdrom',str(ROOT/'build/iso/zionos-genesis-0.6.1-x86_64.iso'),'-drive',f'file={media},media=cdrom,readonly=on,index=3',
                 '-drive',f'file={disk},if=none,id=sdk-test-disk,format=qcow2,cache=none','-device','virtio-blk-pci,drive=sdk-test-disk,serial=ZPKSDKTEST01',
                 '-nic','user,model=e1000,hostfwd=tcp:127.0.0.1:8080-:8080','-fw_cfg','name=opt/zionos/network,string=QEMU-USERNET-LOCALHOST-V1',
                 '-boot','d','-serial','mon:stdio','-display','none','-no-reboot']
        process=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        selector=selectors.DefaultSelector();selector.register(process.stdout,selectors.EVENT_READ)
        log=bytearray();stage='login';deadline=time.monotonic()+600
        def send(text):process.stdin.write(text.encode()+b'\n');process.stdin.flush()
        def browser(*args):subprocess.run(['node',str(ROOT/'tests/apps/browser.cjs'),*args],cwd=ROOT,env={**os.environ,'ZION_SDK_ORIGIN':'http://127.0.0.1:8080'},check=True)
        try:
            while time.monotonic()<deadline:
                for key,_ in selector.select(.2):
                    data=key.fileobj.read1(65536)
                    if not data: raise RuntimeError('SDK VM exited early')
                    log.extend(data)
                if b'Kernel panic - not syncing' in log: raise RuntimeError('kernel panic')
                if stage=='login' and b'zionos login:' in log:send('root');stage='shell'
                elif stage=='shell' and b'zionos:~#' in log:
                    send('mkdir -p /mnt/zion-sdk-fixtures; mount -r -t iso9660 /dev/sr1 /mnt/zion-sdk-fixtures; sh /mnt/zion-sdk-fixtures/guest.sh');stage='setup'
                elif stage=='setup' and b'\nZION_SDK_VM_READY\r\n' in log:
                    browser()
                    # A second window never creates a second backend. The same supervised PID survives.
                    send('test -d "/proc/$(cat /tmp/zion-sdk-native-pid)" && zpk uninstall org.zionos.zion-hello && test ! -d "/proc/$(cat /tmp/zion-sdk-native-pid)" && echo ZION_SDK_VM_REMOVED');stage='removed'
                elif stage=='setup' and b'\nZION_SDK_VM_SETUP_DONE\r\n' in log:raise RuntimeError('SDK guest setup failed; see build/sdk-vm.log')
                elif stage=='removed' and b'\nZION_SDK_VM_REMOVED\r\n' in log:
                    browser('--removed');send('sync; poweroff');process.wait(timeout=45)
                    if process.returncode!=0:raise RuntimeError('unclean SDK VM shutdown')
                    print('PASS native SDK VM: compiled Forge backend/non-root OpenRC, automatic desktop discovery/bridge/isolation, uninstall revocation');return
                if process.poll() is not None:raise RuntimeError('SDK VM exited')
            raise TimeoutError('SDK VM '+stage)
        finally:
            selector.close()
            if process.poll() is None:
                process.terminate()
                try:process.wait(timeout=10)
                except subprocess.TimeoutExpired:process.kill();process.wait()
            (ROOT/'build/sdk-vm.log').write_bytes(log)

if __name__=='__main__':main()
