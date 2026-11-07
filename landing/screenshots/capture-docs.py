"""Supporting docs captures. Pass a device_open bound CLI and ipad or iphone.
May also pass a named stage to resume after inspecting a failed capture.
"""
import pathlib, re, subprocess, sys, time
agent, device = sys.argv[1:3]
stage = sys.argv[3] if len(sys.argv)>3 else 'all'
root = pathlib.Path(__file__).resolve().parents[2]
out = root/'landing/public/screenshots/ipad-2026-11-07/raw'
out.mkdir(parents=True,exist_ok=True)
def run(*args):
    return subprocess.check_output([agent,*map(str,args)],text=True)
def press(prefix):
    snap=run('snapshot','-i')
    for line in snap.splitlines():
        if '[button]' in line and ('"'+prefix) in line:
            return run('press',re.search(r'@e\d+(?:~s\d+)?',line).group(),'--settle')
    raise RuntimeError('Missing '+prefix+'\n'+snap)
def shot(name):
    snap=run('snapshot','-i')
    if name == 'layers' and 'New from template' not in snap:
        raise RuntimeError('Layers sheet did not open; refusing a bank-menu capture')
    if re.search(r'Demo(?: desk|[ ·]+simulated)',snap,re.I):
        raise RuntimeError('Redundant badge visible: '+name)
    print(run('screenshot',out/(name+'-raw.png'),'--pixel-density','3' if device=='iphone' else '2'),flush=True)
def home(): press('OpenFader')
def tool(name):
    home(); press('Tools'); press(name)
if stage in ('all','start'):
    run('open','com.openfader.flutter.experimental','--relaunch')
    run('orientation','portrait' if device=='iphone' else 'landscape-left')
    if device=='ipad':
        shot('start-connections'); press('Add manually'); shot('connect-add')
        press('Close')
    press('Work offline'); time.sleep(12)
if device=='iphone':
    shot('mixer-phone'); press('Kick EQ and processing'); shot('channel-phone')
else:
    if stage in ('all','channel'):
        press('Kick EQ and processing'); shot('channel-input')
        press('Gate processing'); shot('channel-gate')
        press('Comp processing'); shot('channel-compressor')
    if stage in ('all','meters'):
        tool('Meters'); shot('meters')
    if stage in ('all','settings'):
        home(); press('Settings'); shot('settings')
        press('Access limits'); shot('settings-access')
    if stage in ('all','recorder'):
        home(); press('Tools')
        if 'Recorder' in run('snapshot','-i'):
            press('Recorder'); shot('recorder')
        else:
            print('BLOCKED recorder: native iOS offline session has no computer HostTransport',flush=True)
    if stage in ('all','banks'):
        tool('Mixer layout'); press('FxRet 1-4'); home(); shot('mixer-banks')
        tool('Mixer layout')
        # The menu exposes off-screen AX refs; scroll the actual menu first.
        run('gesture','pan','1125','660','0','-300','700')
        time.sleep(2)
        press('Layers'); shot('layers')
    if stage in ('all','banks','sends'):
        # AX Dismiss targets the sheet centre; use the observed exterior scrim.
        run('press','100','200','--settle'); tool('Mixer layout'); press('All'); home()
        press('BUS1'); shot('mixer-sends')
subprocess.check_call([sys.executable,str(pathlib.Path(__file__).with_name('export-docs.py'))])
