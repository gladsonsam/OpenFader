"""Export only genuine captures; a missing raw is never replaced with old media."""
from pathlib import Path
import json, shutil, subprocess, sys
from PIL import Image
root=Path(__file__).resolve().parents[2]
base=root/'landing/public/screenshots/ipad-2026-11-07'
raw=base/'raw'
subprocess.check_call([sys.executable,str(Path(__file__).with_name('frame-ipad.py')),str(raw),str(base)])
names='start-connections connect-add mixer-banks layers mixer-sends mixer-phone meters channel-input channel-gate channel-compressor channel-phone recorder settings settings-access'.split()
rows={}
for name in names:
    path=raw/(name+'-raw.png')
    if name=='recorder':
        rows[name]={'status':'blocked','reason':'Native offline iOS has no computer HostTransport; recorder route is unavailable.'}
        continue
    if not path.exists():
        rows[name]={'status':'not captured'}
        continue
    im=Image.open(path)
    if name=='connect-add':
        target=base/(name+'-detail.png')
        im.crop((896,604,1856,1484)).save(target)
    elif name in ('mixer-phone','channel-phone'):
        target=base/(name+'-detail.png');shutil.copyfile(path,target)
    else: target=base/(name+'-framed.png')
    rows[name]={'status':'captured','raw':'/screenshots/ipad-2026-11-07/raw/'+path.name,'raw_dimensions':list(im.size),'asset':'/screenshots/ipad-2026-11-07/'+target.name,'asset_dimensions':list(Image.open(target).size),'capture_padding':0}
(base/'docs-assets.json').write_text(json.dumps(rows,indent=2)+'\n')
