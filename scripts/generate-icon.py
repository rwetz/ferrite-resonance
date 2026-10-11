from pathlib import Path
import xml.etree.ElementTree as ET
from PIL import Image, ImageDraw
root = Path(__file__).resolve().parents[1]
svg = ET.parse(root / "assets/logo.svg").getroot()
image = Image.new("RGB", (24, 24), "#0B0B0C")
draw = ImageDraw.Draw(image)
for rect in svg.findall("{http://www.w3.org/2000/svg}rect"):
    x, y = int(rect.get("x", 0)), int(rect.get("y", 0))
    w, h = int(rect.get("width")), int(rect.get("height"))
    draw.rectangle((x, y, x + w - 1, y + h - 1), fill=rect.get("fill"))
image.resize((256, 256), Image.Resampling.NEAREST).save(
    root / "assets/logo.ico",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
