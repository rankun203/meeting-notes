"""Regenerate the Finder artwork: uv run --no-project --with pillow this_file.py."""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


SCALE = 3
image = Image.new("RGB", (400 * SCALE, 472 * SCALE), "#f4faf9")
draw = ImageDraw.Draw(image)
fonts = Path("/System/Library/Fonts/Supplemental")


def text(y, content, size, color, bold=False):
    font = ImageFont.truetype(str(fonts / ("Arial Bold.ttf" if bold else "Arial.ttf")), size * SCALE)
    draw.text((200 * SCALE, y * SCALE), content, font=font, fill=color, anchor="mt")


text(12, "Install Gday Meetings", 20, "#075e65", bold=True)
# The real Finder icons sit at (100, 120) and (280, 120).
draw.line([(166 * SCALE, 120 * SCALE), (214 * SCALE, 120 * SCALE)], fill="#13858a", width=4 * SCALE)
draw.line([(202 * SCALE, 108 * SCALE), (214 * SCALE, 120 * SCALE), (202 * SCALE, 132 * SCALE)], fill="#13858a", width=4 * SCALE, joint="curve")
text(246, "Drag the app into Applications", 17, "#075e65", bold=True)
text(276, "Then open it from Applications.", 13, "#507174")

output = Path(__file__).resolve().with_name("installer-background.tiff")
# Preserve sharp text on Retina displays at a logical size of 400 x 472 points.
image.save(output, dpi=(72 * SCALE, 72 * SCALE), compression="tiff_lzw")
