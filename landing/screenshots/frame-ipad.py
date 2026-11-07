"""Compose full iPad captures at native size, without cropping or resizing UI."""
from pathlib import Path
import sys
from PIL import Image, ImageDraw

source, destination = map(Path, sys.argv[1:3])
destination.mkdir(parents=True, exist_ok=True)
for path in sorted(source.glob('*.png')):
    ui = Image.open(path).convert('RGBA')
    w, h = ui.size
    bezel, margin = 48, 24
    frame = Image.new('RGBA', (w + 2*(bezel+margin), h + 2*(bezel+margin)))
    draw = ImageDraw.Draw(frame)
    draw.rounded_rectangle((margin, margin, frame.width-margin-1, frame.height-margin-1), 76, fill='#17191c', outline='#45484e', width=3)
    frame.alpha_composite(ui, (margin+bezel, margin+bezel))
    draw.ellipse((margin+14, frame.height//2-7, margin+28, frame.height//2+7), fill='#08090b')
    frame.save(destination / path.name.replace('-raw', '-framed'))
