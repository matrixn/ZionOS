"""Disposable QEMU test only: no host services/accounts, disks or runtime containers."""
from pathlib import Path
import selectors,subprocess,time,os,fcntl
ROOT=Path(__file__).resolve().parents[2]

def main():
    kvm=False
    try:
        with open('/dev/kvm','r+b',buffering=0) as device:
            vm=fcntl.ioctl(device.fileno(),0xae01,0);os.close(vm);kvm=True
    except OSError:pass
    accel=os.environ.get('ZPK_RUNTIME_ACCEL','kvm' if kvm else 'tcg')
    if accel not in ('kvm','tcg'):raise ValueError('ZPK_RUNTIME_ACCEL must be kvm or tcg')
    command=['qemu-system-x86_64','-accel',accel,'-cpu','host' if accel=='kvm' else 'qemu64','-m','2048','-smp','2',
             '-cdrom',str(Path(subprocess.check_output(["sh",str(ROOT/"scripts/iso-path.sh")],text=True).strip())),
             '-drive',f'file={ROOT}/build/runtime-fixtures.iso,media=cdrom,readonly=on,index=3',
             '-nic','user,model=e1000','-boot','d','-serial','mon:stdio','-display','none','-snapshot']
    process=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    selector=selectors.DefaultSelector();selector.register(process.stdout,selectors.EVENT_READ)
    log=bytearray();phase='login';deadline=time.monotonic()+600
    try:
        while time.monotonic()<deadline:
            for key,_ in selector.select(.2):
                data=key.fileobj.read1(65536)
                if not data:raise RuntimeError('VM exited')
                log.extend(data)
            if b'Kernel panic - not syncing' in log:raise RuntimeError('kernel panic')
            if phase=='login' and b'zionos login:' in log:
                process.stdin.write(b'root\n');process.stdin.flush();phase='shell'
            elif phase=='shell' and b'zionos:~#' in log:
                process.stdin.write(b'mkdir -p /mnt/runtime-fixtures; mount -r -t iso9660 /dev/sr1 /mnt/runtime-fixtures; sh /mnt/runtime-fixtures/guest.sh\n');process.stdin.flush();phase='test'
            elif phase=='test' and b'\nZPK_RUNTIME_VM_DONE\r\n' in log:
                if b'ZPK_RUNTIME_VM_PASS' not in log:raise RuntimeError('runtime tests failed; see build/runtime-vm.log')
                print('PASS: native OpenRC runtime VM integration');return
            if process.poll() is not None:raise RuntimeError('VM exited')
        raise TimeoutError('runtime VM stage '+phase)
    finally:
        selector.close();process.terminate()
        try:process.wait(timeout=10)
        except subprocess.TimeoutExpired:process.kill();process.wait()
        (ROOT/'build/runtime-vm.log').write_bytes(log)

if __name__=='__main__':main()
