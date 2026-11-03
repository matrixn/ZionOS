"""Deterministic owned icon + SDK distribution copy; no network or image dependencies."""
from pathlib import Path
import shutil,struct,zlib
ROOT=Path(__file__).resolve().parent
def chunk(kind,data):return struct.pack('!I',len(data))+kind+data+struct.pack('!I',zlib.crc32(kind+data)&0xffffffff)
def main():
    rows=[]
    for y in range(64):
        row=bytearray([0])
        for x in range(64):
            corner=(x<12 and y<12 and (x-12)**2+(y-12)**2>144) or (x>51 and y<12 and (x-51)**2+(y-12)**2>144) or (x<12 and y>51 and (x-12)**2+(y-51)**2>144) or (x>51 and y>51 and (x-51)**2+(y-51)**2>144)
            z=(16<=x<=48 and (17<=y<=23 or 41<=y<=47)) or (23<y<41 and abs(x-(65-y))<=4)
            color=(240,255,252,255) if z else (int(35+y/2),int(145+y/2),int(142+y/3),0 if corner else 255)
            row.extend(color)
        rows.append(row)
    png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('!IIBBBBB',64,64,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(rows),9))+chunk(b'IEND',b'')
    (ROOT/'assets/icon.png').write_bytes(png)
    shutil.copyfile(ROOT/'../../web/zion-app-sdk/dist/zion-app-sdk.mjs',ROOT/'assets/zion-app-sdk.mjs')
if __name__=='__main__':main()
