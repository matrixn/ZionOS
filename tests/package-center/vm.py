"""Production ISO boot/mount/recovery + real authenticated graphical package lifecycle."""
from pathlib import Path
import fcntl
import importlib.util
import json
import os
import re
import secrets
import selectors
import shutil
import signal
import socket
import stat
import subprocess
import sys
import time
import urllib.request
import uuid

ROOT=Path(__file__).resolve().parents[2]
SPEC=importlib.util.spec_from_file_location('images',ROOT/'tests/e2e/images.py')
images=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(images)

def status():
    with urllib.request.urlopen('http://127.0.0.1:8080/api/auth/status',timeout=5) as r:
        return json.load(r)['data']

def boot(folder,phase,interactive=False,resume=False,missing=False,degraded=False):
    config=json.loads((folder/'volume.json').read_text())
    if config.get('format')!='ZION_CENTER_V1' or not re.fullmatch('[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}',config.get('uuid','')):raise ValueError('unrecognized persistent test environment')
    disk=folder/'persistent.qcow2'
    m=disk.lstat()
    if not stat.S_ISREG(m.st_mode) or m.st_nlink!=1 or m.st_uid!=os.geteuid() or m.st_ino!=config['inode'] or m.st_dev!=config['device']:
        raise ValueError('persistent test image identity changed')
    subprocess.run([sys.executable,str(ROOT/'scripts/check-qcow2.py'),str(disk)],check=True)
    with socket.socket() as probe:
        probe.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
        probe.bind(('127.0.0.1',8080))
    legacy=os.environ.get('ZION_CENTER_LEGACY','0')
    if legacy not in ('0','1'):raise ValueError('ZION_CENTER_LEGACY must be 0 or 1')
    kvm=False
    try:
        with open('/dev/kvm','r+b',buffering=0) as f:
            vm=fcntl.ioctl(f.fileno(),0xae01,0);os.close(vm);kvm=legacy!='1'
    except OSError:pass
    version=re.search(r'^version = "([^"]+)"',(ROOT/'core/api/Cargo.toml').read_text(),re.M)[1]
    command=['qemu-system-x86_64','-accel','kvm' if kvm else 'tcg','-cpu','Conroe' if legacy=='1' else 'host' if kvm else 'qemu64','-m','2048','-smp','2',
             '-cdrom',str(ROOT/f'build/iso/zionos-genesis-{version}-x86_64.iso'),
             '-drive',f'file={folder}/fixtures.iso,media=cdrom,readonly=on,index=3',
             '-drive',f'file={disk},if=none,id=center-disk,format=qcow2,cache=none',
             '-device','virtio-blk-pci,drive=center-disk,serial=ZIONCENTER01',
             '-nic','user,model=e1000,hostfwd=tcp:127.0.0.1:8080-:8080',
             '-fw_cfg','name=opt/zionos/network,string=QEMU-USERNET-LOCALHOST-V1',
             '-boot','d','-serial','mon:stdio','-display','none','-no-reboot']
    if not missing:command+=['-fw_cfg',f'name=opt/zionos/apps-uuid,string={config["uuid"]}']
    assert '-snapshot' not in command
    (folder/f'qemu-{phase}.json').write_text(json.dumps(command,indent=2)+'\n')
    proc=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True)
    sel=selectors.DefaultSelector();sel.register(proc.stdout,selectors.EVENT_READ)
    log=bytearray();stage='login';deadline=time.monotonic()+1200
    def send(text):proc.stdin.write(text.encode()+b'\n');proc.stdin.flush()
    def action(name):send(f'/mnt/zion-center-fixtures/guest-check {name}; echo ZION_CENTER_DONE_{name}')
    def completed(name):
        if f'\nZION_CENTER_DONE_{name}\r\n'.encode() not in log:return False
        if f'\nZION_CENTER_PASS_{name}\r\n'.encode() not in log:raise RuntimeError(f'native verifier failed: {name}; inspect VM log')
        return True
    def browser(name):subprocess.run(['node',str(ROOT/'tests/package-center/browser.cjs'),str(folder),name],cwd=ROOT,check=True)
    try:
        while time.monotonic()<deadline:
            for key,_ in sel.select(.2):
                data=key.fileobj.read1(65536)
                if not data and stage!='shutdown':raise RuntimeError('Package Center VM exited early')
                log.extend(data);(folder/f'vm-{phase}.log').write_bytes(log)
            if b'Kernel panic - not syncing' in log:raise RuntimeError('kernel panic')
            if stage=='login' and b'zionos login:' in log:send('root');stage='shell'
            elif stage=='shell' and b'zionos:~#' in log:
                send('mkdir -p /mnt/zion-center-fixtures; mount -r -t iso9660 /dev/sr1 /mnt/zion-center-fixtures; echo ZION_CENTER_MEDIA_READY');stage='ready'
            elif stage=='ready' and b'\nZION_CENTER_MEDIA_READY\r\n' in log:
                current=status()
                if missing or degraded:
                    assert current['available'] is False
                    with urllib.request.urlopen('http://127.0.0.1:8080/api/health') as r:assert json.load(r)['status']=='ok'
                    subprocess.run(['node',str(ROOT/'tests/package-center/degraded.cjs'),str(folder),phase],check=True,cwd=ROOT)
                    print('PASS production degraded boot: '+phase+', no RAM fallback, monitoring remains healthy',flush=True)
                    send('sync; poweroff');stage='shutdown';continue
                if not current.get('available'):raise RuntimeError('production persistent platform unavailable: '+str(current.get('detail')))
                if not current.get('configured'):
                    send('cp /mnt/zion-center-fixtures/setup-token /run/zionos/setup-token; chmod 600 /run/zionos/setup-token; /opt/zionos/bin/zion-platform bootstrap-file /run/zionos/setup-token; echo ZION_CENTER_BOOTSTRAP_DONE');stage='bootstrap'
                else:stage='browser'
            elif stage=='bootstrap' and b'\nZION_CENTER_BOOTSTRAP_DONE\r\n' in log:stage='browser'
            if stage=='browser':
                if interactive:
                    print(f'Genesis 0.7 ready: http://127.0.0.1:8080 — open Package Center. One-time setup code is in {folder}/setup-code.txt (private file).',flush=True)
                    print('Enter quit or Ctrl+C to shut down cleanly. Resume this same disk with: bash scripts/run-qemu-package-center.sh --resume '+str(folder),flush=True)
                    sel.register(sys.stdin,selectors.EVENT_READ)
                    while True:
                        for key,_ in sel.select(.5):
                            if key.fileobj is sys.stdin:
                                line=sys.stdin.readline()
                                if not line or line.strip()=='quit':raise KeyboardInterrupt
                                print('Use the graphical Package Center. Terminal accepts quit only.',flush=True);continue
                            data=key.fileobj.read1(65536)
                            if not data:raise RuntimeError('interactive VM exited')
                            log.extend(data)
                browser(phase)
                action('first' if phase=='first' else 'upgraded' if phase=='second' else 'uninstalled')
                stage='verified'
            elif stage=='verified' and completed('first' if phase=='first' else 'upgraded' if phase=='second' else 'uninstalled'):
                if phase=='first':action('ipc-negative');stage='ipc'
                elif phase=='second':
                    send('cp /mnt/zion-center-fixtures/low-space.json /etc/zionos/package-policy.json; rc-service zion-package-broker restart; echo ZION_CENTER_LOW_SPACE_READY');stage='capacity'
                else:
                    send('rc-service zion-package-broker stop && /mnt/zion-center-fixtures/guest-check corrupt-registry; echo ZION_CENTER_DONE_corrupt-registry');stage='corrupted'
            elif stage=='ipc' and completed('ipc-negative'):send('sync; poweroff');stage='shutdown'
            elif stage=='capacity' and b'\nZION_CENTER_LOW_SPACE_READY\r\n' in log:
                browser('capacity');send('rm /etc/zionos/package-policy.json; rc-service zion-package-broker restart; rc-service zion-package-broker stop; /mnt/zion-center-fixtures/guest-check interrupt; echo ZION_CENTER_INTERRUPTED');stage='interrupted'
            elif stage=='interrupted' and b'\nZION_CENTER_INTERRUPTED\r\n' in log:
                if b'\nZION_CENTER_PASS_interrupt\r\n' not in log:raise RuntimeError('interrupted transaction fixture failed')
                send('sync; poweroff');stage='shutdown'
            elif stage=='corrupted' and completed('corrupt-registry'):
                send('sync; poweroff');stage='shutdown'
            if stage=='shutdown' and proc.poll() is not None:
                if proc.returncode!=0:raise RuntimeError('unclean VM shutdown')
                print(f'PASS production Package Center {phase} boot',flush=True);return
            if stage!='shutdown' and proc.poll() is not None:raise RuntimeError('VM stopped unexpectedly')
        raise TimeoutError('Package Center stage '+stage)
    except KeyboardInterrupt:
        send('sync; poweroff');end=time.monotonic()+45
        while proc.poll() is None and time.monotonic()<end:
            for key,_ in sel.select(.2):log.extend(key.fileobj.read1(65536))
        if proc.poll() is None or proc.returncode!=0:raise RuntimeError('clean interactive shutdown failed')
        if not interactive:raise
    finally:
        if proc.poll() is None and stage not in ('login','shell'):
            # Preserve diagnostics and shut down the owned guest cleanly even on a test failure.
            send('cat /var/log/zion-package-broker.err; zpk status org.zionos.zion-hello; zpk logs org.zionos.zion-hello --stderr; sync; poweroff')
            until=time.monotonic()+45
            while proc.poll() is None and time.monotonic()<until:
                for key,_ in sel.select(.2):
                    if key.fileobj is not sys.stdin:log.extend(key.fileobj.read1(65536))
        sel.close()
        if proc.poll() is None:
            proc.terminate()
            try:proc.wait(timeout=10)
            except subprocess.TimeoutExpired:proc.kill();proc.wait()
        (folder/f'vm-{phase}.log').write_bytes(log)

def main():
    def interrupted(_signum,_frame):raise KeyboardInterrupt
    signal.signal(signal.SIGTERM,interrupted)
    folder=images.workspace(sys.argv[1]);mode=sys.argv[2]
    if mode=='--resume':boot(folder,'interactive-resume',interactive=True,resume=True);return
    if mode not in ('--vm','--interactive'):raise ValueError('unsupported VM mode')
    # Credentials are fresh per disposable VM; never print, put in argv, or commit them.
    values={'code':secrets.token_hex(32),'password':secrets.token_hex(24)}
    for name,body in [('credentials.json',json.dumps(values)),('setup-code.txt',values['code'])]:
        path=folder/name
        with path.open('x') as f:os.fchmod(f.fileno(),0o600);f.write(body)
    media=folder/'media';(media/'setup-token').write_text(values['code']);(media/'setup-token').chmod(0o600)
    (media/'low-space.json').write_text(json.dumps({'max_upload':67108864,'reserve_bytes':1073741824}))
    release=Path(os.environ['CARGO_TARGET_DIR'])/'x86_64-unknown-linux-musl/release'
    shutil.copyfile(release/'zion-center-guest',media/'guest-check');(media/'guest-check').chmod(0o755)
    subprocess.run([str(release/'zion-center-guest'),'fixtures',str(folder)],cwd=ROOT,check=True)
    subprocess.run(['xorriso','-as','mkisofs','-R','-J','-o',str(folder/'fixtures.iso'),str(media)],check=True)
    application_uuid=str(uuid.uuid4());seed=folder/'filesystem-seed';seed.mkdir(mode=0o700)
    (seed/'.zionos-app-volume').write_text(f'ZIONOS_APPS_V1\n{application_uuid}\n')
    disk=images.create_image(folder,application_uuid,seed)
    st=disk.stat();(folder/'volume.json').write_text(json.dumps({'uuid':application_uuid,'inode':st.st_ino,'device':st.st_dev,'format':'ZION_CENTER_V1'},indent=2))
    print('Package Center evidence: '+str(folder),flush=True)
    if mode=='--interactive':boot(folder,'first',interactive=True)
    else:
        boot(folder,'missing',missing=True)
        for phase in ['first','second','third']:boot(folder,phase)
        boot(folder,'corrupt-registry',degraded=True)
        invalid=folder/'invalid-volume';invalid.mkdir()
        shutil.copyfile(folder/'fixtures.iso',invalid/'fixtures.iso')
        bad_uuid=str(uuid.uuid4());seed=invalid/'filesystem-seed';seed.mkdir(mode=0o700)
        (seed/'.zionos-app-volume').write_text('WRONG_VOLUME_ROLE\n')
        disk=images.create_image(invalid,bad_uuid,seed);st=disk.stat()
        (invalid/'volume.json').write_text(json.dumps({'uuid':bad_uuid,'inode':st.st_ino,'device':st.st_dev,'format':'ZION_CENTER_V1'}))
        boot(invalid,'invalid-volume',degraded=True)
        print('PASS Genesis 0.7 complete authenticated graphical native lifecycle across persistent reboots',flush=True)

if __name__=='__main__':main()
