#!/usr/bin/env python3
"""Generate our own vector TGS and transparent VP9 sticker smoke fixtures."""
import gzip, json, math, pathlib, struct, subprocess, tempfile, zlib
root = pathlib.Path(__file__).resolve().parents[1]
out = root / 'docs/screenshots/fixtures'
out.mkdir(parents=True, exist_ok=True)
value = {'v':'5.7.0','tgs':1,'w':128,'h':128,'fr':30,'ip':0,'op':60,'assets':[],
 'layers':[{'ty':4,'ind':1,'ip':0,'op':60,'st':0,'sr':1,'ks':{
 'o':{'a':0,'k':100},'r':{'a':0,'k':0},'a':{'a':0,'k':[0,0,0]},'s':{'a':0,'k':[100,100,100]},
 'p':{'a':1,'k':[{'t':0,'s':[32,64,0],'e':[96,64,0],'o':{'x':0.33,'y':0.33},'i':{'x':0.67,'y':0.67}},
 {'t':30,'s':[96,64,0],'e':[32,64,0],'o':{'x':0.33,'y':0.33},'i':{'x':0.67,'y':0.67}}, {'t':60,'s':[32,64,0]}]}},
 'shapes':[{'ty':'el','p':{'a':0,'k':[0,0]},'s':{'a':0,'k':[44,44]}},
 {'ty':'fl','c':{'a':0,'k':[1,0.2,0.1,1]},'o':{'a':0,'k':100},'r':1}]}]}
(out/'demo-sticker.tgs').write_bytes(gzip.compress(json.dumps(value).encode(), mtime=0))
def chunk(kind, data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
with tempfile.TemporaryDirectory(prefix='quill-sticker-') as tmp:
    for frame in range(60):
        xcenter=64+32*math.sin(frame*math.pi/30)
        rows=[]
        for y in range(128):
            row=bytearray([0])
            for x in range(128):
                row.extend((32,160,255,255) if (x-xcenter)**2+(y-64)**2<22**2 else (0,0,0,0))
            rows.append(row)
        png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',128,128,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(rows)))+chunk(b'IEND',b'')
        pathlib.Path(tmp,f'{frame:03}.png').write_bytes(png)
    subprocess.run(['ffmpeg','-y','-hide_banner','-loglevel','error','-framerate','30','-i',f'{tmp}/%03d.png','-c:v','libvpx-vp9','-pix_fmt','yuva420p','-auto-alt-ref','0',str(out/'demo-sticker.webm')],check=True)
print(out)
