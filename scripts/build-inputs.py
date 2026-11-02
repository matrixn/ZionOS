"""Content/mode-based cache keys; missing inputs fail rather than reuse old output."""
import hashlib
import os
from pathlib import Path
import stat
import sys


def digest(files, extra=""):
    result=hashlib.sha256(extra.encode())
    for name,path in sorted(files):
        metadata=path.lstat()
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError(f"cache input is not a regular file: {path}")
        result.update(name.encode()+b"\0")
        result.update(str(stat.S_IMODE(metadata.st_mode)).encode()+b"\0")
        result.update(hashlib.sha256(path.read_bytes()).digest())
    return result.hexdigest()


def kernel(root, keys, builder):
    relative=["platform/alpine/Dockerfile.builder","platform/alpine/mkimg.zionos.sh",
              "platform/alpine/initramfs-banner.sh","platform/alpine/boot-ui.sh",
              "scripts/brand-initramfs.sh","scripts/build-inputs.py"]
    public=sorted(keys.glob("*.rsa.pub"))
    if not public: raise ValueError("builder public signing key missing")
    return digest([(p,root/p) for p in relative]+[("public/"+p.name,p) for p in public],builder)


def overlay(root):
    files=[]
    def failure(error): raise error
    for subtree in [root/"platform/alpine",root/"build/stage"]:
        if not subtree.is_dir(): raise ValueError(f"missing cache input directory: {subtree}")
        for directory,subdirs,names in os.walk(subtree,onerror=failure,followlinks=False):
            if any((Path(directory)/d).is_symlink() for d in subdirs):
                raise ValueError("symlinked cache input directory")
            for name in names:
                path=Path(directory)/name
                files.append((str(path.relative_to(root)),path))
    return digest(files)


if __name__=="__main__":
    root=Path(sys.argv[2]).resolve()
    if sys.argv[1]=="kernel": print(kernel(root,Path(sys.argv[3]),sys.argv[4]))
    elif sys.argv[1]=="overlay": print(overlay(root))
    else: raise SystemExit("mode must be kernel or overlay")
