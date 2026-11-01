"""Generate Story Stage's vector-style icon at Windows and UI sizes."""
from pathlib import Path
from PIL import Image, ImageDraw
ROOT=Path(__file__).resolve().parents[1]
S=4
im=Image.new('RGBA',(256*S,256*S));d=ImageDraw.Draw(im)
def points(ps):return [(round(x*S),round(y*S)) for x,y in ps]
def poly(ps,color):d.polygon(points(ps),fill=color)
def line(ps,color,width):d.line(points(ps),fill=color,width=width*S,joint='curve')
def curve(a,b,c,e):
 return [((1-t)**3*a[0]+3*(1-t)**2*t*b[0]+3*(1-t)*t*t*c[0]+t**3*e[0],(1-t)**3*a[1]+3*(1-t)**2*t*b[1]+3*(1-t)*t*t*c[1]+t**3*e[1]) for t in [i/48 for i in range(49)]]
d.rounded_rectangle((10*S,10*S,246*S,246*S),radius=54*S,fill='#264f45')
poly([(128,36),(78,113),(178,113)],'#527563')
poly([(128,36),(135,52),(151,59),(135,66),(128,82),(121,66),(105,59),(121,52)],'#eac98b')
left=curve((40,105),(72,91),(98,95),(128,113))+[(128,199)]+curve((128,199),(99,181),(69,180),(40,191))
right=curve((128,113),(158,95),(184,91),(216,105))+[(216,191)]+curve((216,191),(187,180),(157,181),(128,199))
poly(left,'#f2e5c9');poly(right,'#fff9e9')
line([(40,197),(92,193),(128,207),(164,193),(216,197)],'#bc9b63',5)
line([(128,115),(128,197)],'#c7aa72',4)
for y in (126,146):
 line([(59,y),(106,y+10)],'#c4b89b',4)
 line([(150,y+10),(197,y)],'#d9cfb5',4)
# Smaller Shannon mark, distinct from the story symbol.
d.rounded_rectangle((186*S,186*S,254*S,254*S),radius=19*S,fill='#f8fafb')
d.rounded_rectangle((191*S,191*S,250*S,250*S),radius=15*S,fill='#344055')
poly([(212,201),(229,201),(240,212),(240,229),(229,240),(212,240),(201,229),(201,212)],'#91bcff')
poly([(214,208),(226,208),(233,215),(233,226),(226,233),(215,233),(208,226),(208,215)],'#344055')
icons=ROOT/'src-tauri'/'icons'
for size in (16,24,32,64,128,256):
 icon=im.resize((size,size),Image.Resampling.LANCZOS);icon.save(icons/f'story-{size}.png')
 if size==64:(icons/'story.rgba').write_bytes(icon.tobytes())
im.resize((256,256),Image.Resampling.LANCZOS).save(ROOT/'public'/'story-icon.png')
preview=Image.new('RGB',(440,160),'#f6f5ef');pd=ImageDraw.Draw(preview);pd.rectangle((220,0,440,160),fill='#202923')
for base in (0,220):
 for x,size in ((26,16),(80,24),(142,32)):
  icon=Image.open(icons/f'story-{size}.png');preview.paste(icon,(base+x,57-size//2),icon);pd.text((base+x,101),f'{size}px',fill='#879181')
preview.save(icons/'story-preview.png')
