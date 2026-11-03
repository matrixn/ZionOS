"""Fresh regular files only; format/convert through pinned descriptors, never devices."""
from pathlib import Path
import os
import stat
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def workspace(folder):
    folder = Path(folder).absolute()
    build = ROOT / 'build'
    if build.is_symlink() or folder.is_symlink() or folder.resolve() != folder:
        raise ValueError('noncanonical image workspace')
    if not folder.is_dir() or not folder.is_relative_to(build.resolve()) or folder == build:
        raise ValueError('image workspace must be a new directory inside build')
    return folder


def create_image(folder, application_uuid=None, seed=None):
    folder = workspace(folder)
    raw, disk = folder / 'filesystem.img', folder / 'persistent.qcow2'
    # Refuse ANY existing entry, including dangling links, before creating either file.
    for path in (raw, disk):
        if os.path.lexists(path):
            raise FileExistsError(f'refusing existing image: {path}')
    with raw.open('x+b') as source, disk.open('x+b') as target:
        before = []
        for file in (source, target):
            metadata = os.fstat(file.fileno())
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
                raise ValueError('fresh image is not an exclusive regular file')
            before.append((metadata.st_dev, metadata.st_ino))
        source.truncate(512 * 1024 * 1024)
        source.flush()
        # The tools receive exactly our freshly created descriptors. No path substitution
        # can redirect filesystem creation onto an existing image or physical device.
        src, dst = f'/proc/self/fd/{source.fileno()}', f'/proc/self/fd/{target.fileno()}'
        args=['mkfs.ext4', '-q', '-F', '-L', 'ZPKE2ETEST']
        if application_uuid is not None:
            import re
            if not re.fullmatch('[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}',application_uuid):
                raise ValueError('invalid application UUID')
            seed=Path(seed).resolve()
            if not seed.is_relative_to(folder) or not seed.is_dir():
                raise ValueError('application seed outside fresh workspace')
            args += ['-U',application_uuid,'-d',str(seed),'-E','root_owner=0:0']
        subprocess.run([*args,src],
                       pass_fds=(source.fileno(),), check=True)
        if application_uuid is not None:
            # These metadata changes apply ONLY to the same freshly created regular file.
            for inode,mode in [('<2>','040700'),('/.zionos-app-volume','0100600')]:
                for field,value in [('uid','0'),('gid','0'),('mode',mode)]:
                    subprocess.run(['debugfs','-w','-R',f'set_inode_field {inode} {field} {value}',src],pass_fds=(source.fileno(),),check=True,stdout=subprocess.DEVNULL)
        subprocess.run(['qemu-img', 'convert', '-f', 'raw', '-O', 'qcow2', src, dst],
                       pass_fds=(source.fileno(), target.fileno()), check=True)
        for path, identity in zip((raw, disk), before):
            metadata = path.lstat()
            if not stat.S_ISREG(metadata.st_mode) or (metadata.st_dev, metadata.st_ino) != identity:
                raise ValueError('image identity changed')
        target.flush()
        os.fsync(target.fileno())
    raw.unlink()
    return disk
