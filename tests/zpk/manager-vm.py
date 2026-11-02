"""Three real boots of a disposable, preformatted virtual filesystem; no host disks."""
from pathlib import Path
import fcntl, os, selectors, stat, subprocess, tempfile, time
ROOT=Path(__file__).resolve().parents[2]

def boot(disk,phase):
    kvm=False
    try:
        with open('/dev/kvm','r+b',buffering=0) as device:
            vm=fcntl.ioctl(device.fileno(),0xae01,0);os.close(vm);kvm=True
    except OSError: pass
    accel=os.environ.get('ZPK_MANAGER_ACCEL','kvm' if kvm else 'tcg')
    if accel not in ('kvm','tcg'): raise ValueError('ZPK_MANAGER_ACCEL must be kvm or tcg')
    command=['qemu-system-x86_64','-accel',accel,'-cpu','host' if accel=='kvm' else 'qemu64','-m','2048','-smp','2',
             '-cdrom',str(ROOT/'build/iso/zionos-genesis-0.6.1-x86_64.iso'),
             '-drive',f'file={ROOT}/build/manager-fixtures.iso,media=cdrom,readonly=on,index=3',
             '-drive',f'file={disk},if=none,id=zpk-test-disk,format=qcow2,cache=none',
             '-device','virtio-blk-pci,drive=zpk-test-disk,serial=ZPKMGRTEST01',
             '-nic','user,model=e1000','-boot','d','-serial','mon:stdio','-display','none','-no-reboot']
    process=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    selector=selectors.DefaultSelector();selector.register(process.stdout,selectors.EVENT_READ)
    log=bytearray();stage='login';deadline=time.monotonic()+600
    try:
        while time.monotonic()<deadline:
            for key,_ in selector.select(.2):
                data=key.fileobj.read1(65536)
                if not data: raise RuntimeError('VM exited before verification')
                log.extend(data)
            if b'Kernel panic - not syncing' in log: raise RuntimeError('kernel panic')
            if stage=='login' and b'zionos login:' in log:
                process.stdin.write(b'root\n');process.stdin.flush();stage='shell'
            elif stage=='shell' and b'zionos:~#' in log:
                process.stdin.write(b'mkdir -p /mnt/zpk-manager-fixtures; mount -r -t iso9660 /dev/sr1 /mnt/zpk-manager-fixtures; sh /mnt/zpk-manager-fixtures/guest.sh\n');process.stdin.flush();stage='test'
            elif stage=='test' and b'\nZPK_MANAGER_VM_DONE\r\n' in log:
                if f'ZPK_MANAGER_VM_PASS_{phase}'.encode() not in log: raise RuntimeError(f'manager {phase} failed; see build/manager-vm-{phase}.log')
                process.stdin.write(b'poweroff\n');process.stdin.flush()
                process.wait(timeout=45)
                if process.returncode!=0: raise RuntimeError('unclean QEMU shutdown')
                print(f'PASS: native manager persistent VM {phase} boot');return
            if process.poll() is not None: raise RuntimeError('VM exited')
        raise TimeoutError(f'manager VM {phase}: {stage}')
    finally:
        selector.close()
        if process.poll() is None:
            process.terminate()
            try: process.wait(timeout=10)
            except subprocess.TimeoutExpired: process.kill();process.wait()
        (ROOT/f'build/manager-vm-{phase}.log').write_bytes(log)

def main():
    with tempfile.TemporaryDirectory(prefix='manager-vm-',dir=ROOT/'build') as temporary:
        folder=Path(temporary).resolve()
        if not folder.is_relative_to((ROOT/'build').resolve()): raise ValueError('unexpected image workspace')
        raw=folder/'filesystem.img';disk=folder/'persistent.qcow2'
        with raw.open('xb') as output:
            if not stat.S_ISREG(os.fstat(output.fileno()).st_mode): raise ValueError('not a regular file')
            output.truncate(512*1024*1024)
        # This tool sees ONLY the newly created regular file, never a block device.
        subprocess.run(['mkfs.ext4','-q','-F','-L','ZPKMGRTEST',str(raw)],check=True)
        subprocess.run(['qemu-img','convert','-f','raw','-O','qcow2',str(raw),str(disk)],check=True)
        raw.unlink()
        boot(disk,'first');boot(disk,'second');boot(disk,'third')
    print('PASS: install/start/upgrade/rollback/reboot/uninstall/reinstall of native Forge packages')

if __name__=='__main__': main()
