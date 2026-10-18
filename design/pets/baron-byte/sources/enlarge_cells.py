"""Fit the existing approved pet pixels larger; no new character artwork."""
from pathlib import Path
from PIL import Image
import json, math, importlib.util, sys
run=Path(sys.argv[1]); atlas=Image.open(run/"downloaded.png").convert("RGBA")
scale=1.28; baseline=202
specs=[("idle",6),("running-right",8),("running-left",8),("waving",4),("jumping",5),("failed",8),("waiting",6),("running",6),("review",6),("look-row-9",8),("look-row-10",8)]
root=run/"frames"; root.mkdir(exist_ok=True)
manifest={"ok":True,"chroma_key":{"hex":"#FF00FF","rgb":[255,0,255],"threshold":96},"rows":[]}
report={"scale":scale,"original_idle_height":141,"baseline":baseline,"transforms":[]}
for row,(state,count) in enumerate(specs):
 folder=root/state; folder.mkdir(exist_ok=True); paths=[]
 for col in range(count):
  old=atlas.crop((col*192,row*208,(col+1)*192,(row+1)*208))
  bounds=old.getchannel("A").point(lambda x:255 if x>16 else 0).getbbox()
  enlarged=old.resize((round(192*scale),round(208*scale)),Image.Resampling.LANCZOS)
  left=round(96-96*scale)
  if state=="jumping":
   original_foot=bounds[3]
   desired_foot=baseline-round((baseline-original_foot)*12/52)
   desired_foot=max(desired_foot,math.ceil((bounds[3]-bounds[1])*scale)+5)
   top=round(desired_foot-original_foot*scale)
  else:
   top=round(baseline-baseline*scale)
  cell=Image.new("RGBA",(192,208)); cell.alpha_composite(enlarged,(left,top))
  b=cell.getchannel("A").point(lambda x:255 if x>16 else 0).getbbox()
  assert b[0]>=3 and b[1]>=3 and b[2]<=189 and b[3]<=205,(state,col,b)
  path=folder/f"{col:02d}.png"; cell.save(path); paths.append(str(path))
  report["transforms"].append({"row":row,"frame":col,"scale":scale,"translation":[left,top],"before":bounds,"after":b})
 if row<9:manifest["rows"].append({"state":state,"method":"components","frames":paths,"provenance":"Exact cell extraction from the previously validated v2 atlas; one common 1.28 scale, with shorter registered jump trajectory."})
(root/"frames-manifest.json").write_text(json.dumps(manifest,indent=2))
(run/"qa").mkdir(exist_ok=True)
(run/"qa/size-registration.json").write_text(json.dumps(report,indent=2))
print(json.dumps({"ok":True,"scale":scale,"new_idle_height":report["transforms"][0]["after"][3]-report["transforms"][0]["after"][1],"frames":len(report["transforms"])}))
