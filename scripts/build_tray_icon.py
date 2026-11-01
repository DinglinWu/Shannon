"""Build the dedicated Windows tray artwork. No external assets or runtime decoders."""
from pathlib import Path
from PIL import Image, ImageDraw
ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'src-tauri' / 'icons'
SCALE = 4
im = Image.new('RGBA', (256*SCALE, 256*SCALE))
d = ImageDraw.Draw(im)
def box(v): return tuple(int(n*SCALE) for n in v)
def poly(points, fill): d.polygon([(int(x*SCALE), int(y*SCALE)) for x,y in points], fill=fill)
def ellipse(v, fill): d.ellipse(box(v), fill=fill)
# Familiar twin conversation bubbles, simplified for 16–32px tray rendering.
poly([(41,133),(29,171),(77,149)], '#07c160')
ellipse((9,25,176,155), '#07c160')
for x in (64,117): ellipse((x-7,73,x+7,87), '#ffffff')
# A darker rim separates the second bubble on both light and dark taskbars.
poly([(181,166),(213,198),(207,156)], '#159b54')
ellipse((86,86,247,204), '#159b54')
poly([(178,161),(205,188),(200,155)], '#ffffff')
ellipse((94,94,239,196), '#ffffff')
for x in (137,187): ellipse((x-6,133,x+6,145), '#128a4a')
# Shannon corner badge: existing slate tile with a light blue octagonal ring.
d.rounded_rectangle(box((173,173,255,255)), radius=23*SCALE, fill='#f4f7fc')
d.rounded_rectangle(box((179,179,251,251)), radius=19*SCALE, fill='#344055')
poly([(204,190),(225,190),(240,204),(240,225),(225,240),(204,240),(190,225),(190,204)], '#91bcff')
poly([(207,199),(222,199),(231,208),(231,222),(222,231),(208,231),(199,222),(199,208)], '#344055')
for size in (16,20,24,32,48,64,128,256):
    icon=im.resize((size,size), Image.Resampling.LANCZOS)
    icon.save(OUT / f'tray-wechat-{size}.png')
    if size==64: (OUT / 'tray-wechat.rgba').write_bytes(icon.tobytes())
