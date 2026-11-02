"""Reject backing/external-data/protocol images before QEMU sees a scratch disk."""
import json
from pathlib import Path
import struct
import subprocess
import sys


def check_header(data):
    if len(data) < 72 or data[:4] != b"QFI\xfb":
        raise ValueError("not a complete QCOW2 header")
    version = struct.unpack_from(">I", data, 4)[0]
    offset, length = struct.unpack_from(">QI", data, 8)
    if version not in (2, 3) or offset or length:
        raise ValueError("backing files and unknown QCOW2 versions are forbidden")
    if version == 3 and (len(data) < 104 or struct.unpack_from(">Q", data, 72)[0]):
        raise ValueError("dirty/corrupt/external-data/unsupported QCOW2 features are forbidden")


def check(path):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or "," in str(path):
        raise ValueError("scratch disk must be a regular, non-symlink local file")
    with path.open("rb") as stream:
        check_header(stream.read(104))
    info = json.loads(subprocess.check_output(
        ["qemu-img", "info", "-f", "qcow2", "--output=json", str(path)], text=True))
    if info.get("format") != "qcow2" or info.get("backing-filename") or info.get("data-file"):
        raise ValueError("backing/external data file is forbidden")
    if not 128 * 1024**2 <= info["virtual-size"] <= 8 * 1024**3:
        raise ValueError("scratch capacity outside 128 MiB..8 GiB")


if __name__ == "__main__":
    try:
        check(sys.argv[1])
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        raise SystemExit(f"QEMU disk denied: {error}")
