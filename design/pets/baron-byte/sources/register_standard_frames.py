"""Normalize approved rows with one fixed scale/translation per row.
Generated pose pixels remain unchanged except for shared resize and placement.
The bundled extractor and compositor remain authoritative.
"""
from pathlib import Path
from PIL import Image
import json, statistics, sys
run=Path(sys.argv[1]); root=run/"qa/frames"; out=run/"qa/registered-frames"; out.mkdir(parents=True,exist_ok=True)
manifest=json.loads((root/"frames-manifest.json").read_text())
def bbox(im):
    return im.getchannel("A").point(lambda x:255 if x>16 else 0).getbbox()
jump=Image.open(root/"jumping/04.png").convert("RGBA"); jb=bbox(jump)
target_height=jb[3]-jb[1]; baseline=jb[3]
reference_index={"idle":0,"running-right":0,"running-left":0,"waving":0,"jumping":4,"failed":7,"waiting":0,"running":0,"review":0}
report={}
for row in manifest["rows"]:
    state=row["state"]; files=sorted((root/state).glob("*.png"))
    ims=[Image.open(f).convert("RGBA") for f in files]
    boxes=[bbox(im) for im in ims]
    ref=boxes[reference_index[state]]
    scale=1.0 if state=="jumping" else target_height/(ref[3]-ref[1])
    source_baseline=max(b[3] for b in boxes)
    width=round(192*scale); height=round(208*scale)
    offset_x=round(96-96*scale); offset_y=round(baseline-source_baseline*scale)
    folder=out/state; folder.mkdir(exist_ok=True)
    paths=[]
    for f,im in zip(files,ims):
        scaled=im.resize((width,height),Image.Resampling.LANCZOS)
        cell=Image.new("RGBA",(192,208))
        cell.alpha_composite(scaled,(offset_x,offset_y))
        path=folder/f.name; cell.save(path); paths.append(str(path))
    row["frames"]=paths
    report[state]={"scale":scale,"offset":[offset_x,offset_y],"source_baseline":source_baseline,"target_baseline":baseline,"reference_height":ref[3]-ref[1],"target_height":target_height}
(out/"frames-manifest.json").write_text(json.dumps(manifest,indent=2))
(run/"qa/row-registration.json").write_text(json.dumps(report,indent=2))
print(json.dumps({"ok":True,"target_height":target_height,"baseline":baseline,"output":str(out)}))
