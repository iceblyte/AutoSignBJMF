import ctypes, struct, zlib, sys
from ctypes import wintypes
u=ctypes.windll.user32; g=ctypes.windll.gdi32
u.SetProcessDPIAware()
h=u.FindWindowW("Shell_TrayWnd",None)
r=wintypes.RECT(); u.GetWindowRect(h,ctypes.byref(r))
w,ht=r.right-r.left, r.bottom-r.top
hdc=u.GetWindowDC(h); mdc=g.CreateCompatibleDC(hdc)
bmp=g.CreateCompatibleBitmap(hdc,w,ht); g.SelectObject(mdc,bmp)
g.BitBlt(mdc,0,0,w,ht,hdc,0,0,0x00CC0020)
class BIH(ctypes.Structure):
    _fields_=[("biSize",wintypes.DWORD),("biWidth",wintypes.LONG),("biHeight",wintypes.LONG),
              ("biPlanes",wintypes.WORD),("biBitCount",wintypes.WORD),("biCompression",wintypes.DWORD),
              ("biSizeImage",wintypes.DWORD),("biXPelsPerMeter",wintypes.LONG),("biYPelsPerMeter",wintypes.LONG),
              ("biClrUsed",wintypes.DWORD),("biClrImportant",wintypes.DWORD)]
bi=BIH(); bi.biSize=ctypes.sizeof(BIH); bi.biWidth=w; bi.biHeight=-ht; bi.biPlanes=1; bi.biBitCount=32
buf=ctypes.create_string_buffer(w*ht*4)
g.GetDIBits(mdc,bmp,0,ht,buf,ctypes.byref(bi),0)
rows=b''
for y in range(ht):
    row=buf.raw[y*w*4:(y+1)*w*4]
    px=b''.join(bytes((row[i+2],row[i+1],row[i],255)) for i in range(0,len(row),4))
    rows+=b'\x00'+px
def chunk(t,d):
    c=struct.pack('>I',len(d))+t+d
    return c+struct.pack('>I',zlib.crc32(t+d)&0xffffffff)
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,ht,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(rows,9))+chunk(b'IEND',b'')
open(sys.argv[1],'wb').write(png)
print('saved', sys.argv[1], w, ht)
