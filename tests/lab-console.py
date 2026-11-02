"""Opt-in QEMU integration: fresh QCOW2, read-only plan, and blank-FS check refusal.

Never invokes apply, mkfs, mount, or a repair command. Not run by CI.
"""
import json
from pathlib import Path
import re
import selectors
import socket
import subprocess
import time
import urllib.request

ROOT=Path(__file__).resolve().parents[1]


def main():
    with socket.socket() as probe:
        probe.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
        probe.bind(("127.0.0.1",8080))
    log=bytearray()
    process=subprocess.Popen(["bash",str(ROOT/"scripts/run-qemu-lab.sh"),"-display","none","-snapshot"],
                             stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    selector=selectors.DefaultSelector(); selector.register(process.stdout,selectors.EVENT_READ)
    started=time.monotonic(); deadline=started+240; phase="login"
    try:
        while time.monotonic()<deadline:
            for key,_ in selector.select(0.2):
                block=key.fileobj.read1(65536)
                if not block: raise RuntimeError("QEMU exited before console checks completed")
                log.extend(block)
            if phase=="login" and b"zionos login:" in log:
                process.stdin.write(b"root\n"); process.stdin.flush(); phase="shell"
            elif phase=="shell" and b"zionos:~#" in log:
                command=("printf '\\nLAUNCH_SERIAL='; cat /run/zionos/lab-serial; "
                         "printf 'VDA_SERIAL='; cat /sys/block/vda/serial; "
                         "printf 'VDB_SERIAL='; cat /sys/block/vdb/serial; "
                         "zion-lab-provision --plan; code=$?; printf '\\nZION_PLAN_STATUS=%s\\n' \"$code\"; "
                         "zion-lab-recover --check; code=$?; printf '\\nZION_RECOVERY_STATUS=%s\\n' \"$code\"; "
                         "printf 'ZION_CONSOLE_DONE\\n'\n")
                process.stdin.write(command.encode()); process.stdin.flush(); phase="checks"
            elif phase=="checks" and re.search(rb"[\r\n]ZION_CONSOLE_DONE\r?\n",log):
                assert re.search(rb"ZION_PLAN_STATUS=0\r?\n",log),"plan refused: inspect log"
                assert re.search(rb"ZION_RECOVERY_STATUS=2\r?\n",log),"blank disk was not refused for filesystem checking"
                assert b'"execution_enabled":false' in log and b'"signature":"blank"' in log
                with urllib.request.urlopen("http://127.0.0.1:8080/api/provisioning",timeout=5) as response:
                    data=json.load(response)
                assert data["inspection_ready"] and not data["execution_enabled"],data
                print(f"PASS: fresh-session identity, pinned read-only plan and empty-FS refusal ({time.monotonic()-started:.1f}s)")
                return
            if process.poll() is not None: raise RuntimeError("QEMU exited")
        raise TimeoutError(f"console stage {phase} did not finish")
    finally:
        selector.close()
        if process.poll() is None:
            process.terminate()
            try: process.wait(timeout=10)
            except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=5)
        (ROOT/"build/061-lab-console.log").write_bytes(log)


if __name__=="__main__":
    main()
