"""Run after installing the fixture build; pass device_open's bound agent-device path.
Requires Python 3. Capture runs exclusively through the owned Device-panel session.
"""
import pathlib, re, subprocess, sys, time
if '--docs' in sys.argv:
    args = [a for a in sys.argv[1:] if a != '--docs']
    subprocess.check_call([sys.executable, str(pathlib.Path(__file__).with_name('capture-docs.py')), *args])
    raise SystemExit(0)
agent = sys.argv[1]
root = pathlib.Path(__file__).resolve().parents[2]
out = root / 'app/assets/screenshots/ipad-2026-11-07/raw'
out.mkdir(parents=True, exist_ok=True)
def run(*args):
    return subprocess.check_output([agent, *map(str, args)], text=True)
def press(prefix):
    snapshot = run('snapshot', '-i')
    for line in snapshot.splitlines():
        if '[button]' in line and ('"' + prefix) in line:
            ref = re.search(r'@e\d+(?:~s\d+)?', line).group()
            print(run('press', ref, *([] if prefix == 'Routing' else ['--settle'])), flush=True)
            if prefix == 'Routing': time.sleep(2)
            return
    raise RuntimeError('Missing button '+prefix+'\n'+snapshot)
def shot(name):
    # Routing's large matrix can exhaust the iOS accessibility watchdog.
    snapshot = '' if name == 'routing' else run('snapshot', '-i')
    if 'Demo desk' in snapshot or 'Demo · simulated' in snapshot:
        raise RuntimeError('Redundant badge is still visible')
    print(run('screenshot', out / (name+'-raw.png'), '--pixel-density', '2'), flush=True)
run('open', 'com.openfader.flutter.experimental', '--relaunch')
run('orientation', 'landscape-left')
press('Work offline')
time.sleep(12)  # allow opt-in offline fixture writes and engine readback
shot('mixing')
run('record', 'start', '--scope', 'device', out.parent / 'mixing-active.mp4')
time.sleep(8)
print(run('record', 'stop'), flush=True)
press('Kick EQ and processing')
press('EQ processing')
shot('channel-processing')
press('OpenFader')
press('Effects')
press('Mute: On')
shot('effects')
press('OpenFader')
press('Tools')
press('Routing')
shot('routing')
# Native iPad 1376x1032 point geometry; matrix AX capture is unavailable.
run('press', '1337', '66', '--settle')
press('Scenes')
shot('scenes')
subprocess.check_call([sys.executable, str(root/'landing/screenshots/frame-ipad.py'), str(out), str(root/'landing/public/screenshots/ipad-2026-11-07')])
gif = out.parent/'mixing-active.gif'
subprocess.check_call(['ffmpeg', '-loglevel', 'error', '-y', '-i', str(out.parent/'mixing-active.mp4'), '-t', '8', '-filter_complex', '[0:v]fps=12,scale=1376:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse', str(gif)])
import shutil
for target in [root/'app/assets/screenshots/mixer-ipad.gif', root/'landing/public/screenshots/mixer-ipad.gif', root/'landing/public/screenshots/ipad-2026-11-07/mixing-active.gif']:
    shutil.copyfile(gif, target)
