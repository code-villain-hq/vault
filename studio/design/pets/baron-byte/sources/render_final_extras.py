from pathlib import Path
from PIL import Image, ImageDraw
import json, sys, importlib.util
run=Path(sys.argv[1])
atlas=Image.open(run/"final/spritesheet-extended.png").convert("RGBA")
scripts=Path("/Users/michael/.codex/plugins/cache/openai-curated-remote/work-pets/0.1.6/skills/create-pet/scripts")
spec=importlib.util.spec_from_file_location("preview",scripts/"render_animation_previews.py")
preview=importlib.util.module_from_spec(spec); spec.loader.exec_module(preview)
out=run/"qa/final-previews"; out.mkdir(parents=True,exist_ok=True)
frames_root=out/"frames"
counts=[6,8,8,4,5,8,6,6,6]
states=list(preview.ROW_DURATIONS)
all_frames=[]; all_times=[]; stills=[]
for row,(state,count) in enumerate(zip(states,counts)):
    folder=frames_root/state; folder.mkdir(parents=True,exist_ok=True)
    for col in range(count):
        cell=atlas.crop((col*192,row*208,(col+1)*192,(row+1)*208))
        cell.save(folder/f"{col:02d}.png")
        card=Image.new("RGB",(240,256),"#141414")
        card.paste(cell,(24,16),cell)
        ImageDraw.Draw(card).text((12,232),state+" / "+str(col+1),fill="#EBEBEB")
        all_frames.append(card); all_times.append(preview.ROW_DURATIONS[state][col])
        if (row,col) in [(0,0),(1,2),(3,1),(4,2),(7,2),(8,2)]:
            stills.append(card)
preview.save_preview(all_frames,all_times,out/"all-states.gif")
jump_seq=[(0,0),(0,1),(0,2),(4,0),(4,1),(4,2),(4,3),(4,4),(0,0),(0,1)]
jump=[atlas.crop((c*192,r*208,(c+1)*192,(r+1)*208)) for r,c in jump_seq]
preview.save_preview(jump,[280,110,110,140,140,140,140,280,280,110],out/"idle-jump-idle.gif")
look=[atlas.crop((c*192,r*208,(c+1)*192,(r+1)*208)) for r in [9,10] for c in range(8)]
preview.save_preview(look,[160]*16,out/"look-loop.gif")
strip=Image.new("RGB",(240*len(stills),256),"#141414")
for i,im in enumerate(stills): strip.paste(im,(240*i,0))
strip.save(out/"motion-stills.png")
print(json.dumps({"ok":True,"frames_root":str(frames_root),"output":str(out)}))
