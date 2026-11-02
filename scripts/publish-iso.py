#!/usr/bin/env python3
"""Replace a published ISO without changing the inode held by a running VM."""
import os
import pathlib
import shutil
import sys
import tempfile


def publish(source, destination):
    destination = pathlib.Path(destination)
    fd, temporary = tempfile.mkstemp(prefix='.iso-', dir=destination.parent)
    try:
        with os.fdopen(fd, 'wb') as output, open(source, 'rb') as input_file:
            shutil.copyfileobj(input_file, output)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(temporary, 0o644)
        os.replace(temporary, destination)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


if __name__ == '__main__':
    publish(*sys.argv[1:])
