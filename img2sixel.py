#!/usr/bin/env python3
"""Encode an image as sixel graphics, optionally beside a banner text template.

Requires Pillow (pip install pillow). Width is in terminal columns; static output
uses explicit cell dimensions rather than querying whichever terminal builds it.
"""

import argparse
import math
from pathlib import Path
import sys

from PIL import Image

ESC = "\x1b"


def positive_int(value):
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def prepare_image(path, columns, cell_width, colors):
    """Resize without changing aspect ratio and quantize to a sixel palette.

    Fully transparent pixels remain unpainted. Partial alpha is composited onto
    black, as in img2ascii.py; sixel has no per-pixel alpha channel.
    """
    with Image.open(path) as source:
        image = source.convert("RGBA")
    width = columns * cell_width
    height = max(1, round(width * image.height / image.width))
    image = image.resize((width, height), Image.Resampling.LANCZOS)
    alpha = image.getchannel("A")
    background = Image.new("RGBA", image.size, (0, 0, 0, 255))
    rgb = Image.alpha_composite(background, image).convert("RGB")
    indexed = rgb.quantize(colors=colors, method=Image.Quantize.MEDIANCUT,
                           dither=Image.Dither.NONE)
    return indexed, alpha


def repeat_runs(values):
    """Encode repeated sixel columns only when the repeat command is shorter."""
    output = []
    start = 0
    while start < len(values):
        end = start + 1
        while end < len(values) and values[end] == values[start]:
            end += 1
        character = chr(63 + values[start])
        count = end - start
        repeat = f"!{count}{character}"
        output.append(repeat if len(repeat) < count else character * count)
        start = end
    return "".join(output)


def encode_sixel(image, alpha):
    """Emit DEC sixel: square pixels, transparent background, RGB percentages.

    Protocol: https://vt100.net/docs/vt3xx-gp/chapter14.html
    """
    width, height = image.size
    pixels = image.load()
    opacity = alpha.load()
    palette = image.getpalette()
    # P2=1 leaves pixels not present in the color planes untouched.
    output = [f'{ESC}P0;1;0q"1;1;{width};{height}']
    for color, _ in sorted((index, count) for count, index in image.getcolors()):
        red, green, blue = [round(value * 100 / 255)
                            for value in palette[color * 3:color * 3 + 3]]
        output.append(f"#{color};2;{red};{green};{blue}")

    for top in range(0, height, 6):
        planes = {}
        for y in range(top, min(top + 6, height)):
            bit = 1 << (y - top)
            for x in range(width):
                if opacity[x, y] == 0:
                    continue
                color = pixels[x, y]
                if color not in planes:
                    planes[color] = bytearray(width)
                planes[color][x] |= bit
        for i, (color, values) in enumerate(sorted(planes.items())):
            if i:
                output.append("$")  # Sixel carriage return: repaint this band.
            while values and values[-1] == 0:
                values.pop()
            output.append(f"#{color}{repeat_runs(values)}")
        if top + 6 < height:
            output.append("-")  # Advance one six-pixel band, including empty bands.
    output.append(f"{ESC}\\")
    return "".join(output)


def read_text(path):
    lines = Path(path).read_text(encoding="utf-8").splitlines()
    lines = [line.rstrip().replace("\\x1b", ESC) for line in lines]
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    return lines


def compose(sixel, image_rows, text, columns, gap, valign):
    """Reserve screen rows, draw the image, then place text to its right.

    Cursor save/restore makes layout independent of the terminal's cursor
    position after the sixel DCS. Reserving rows first keeps the whole banner
    above the bottom margin when the terminal scrolls.
    """
    rows = max(image_rows, len(text))

    def offset(height):
        return {"top": 0, "center": (rows - height) // 2,
                "bottom": rows - height}[valign]

    output = ["\r\n" * rows, f"{ESC}[{rows}A\r{ESC}7"]
    image_offset = offset(image_rows)
    if image_offset:
        output.append(f"{ESC}[{image_offset}B")
    output.extend([sixel, f"{ESC}8"])
    text_offset = offset(len(text))
    for row in range(rows):
        index = row - text_offset
        if 0 <= index < len(text) and text[index]:
            output.extend([f"{ESC}[{columns + gap}C", text[index]])
        output.append("\r\n")
    return "".join(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", help="path to the input image")
    parser.add_argument("-w", "--width", type=positive_int, default=80,
                        help="image width in terminal columns (default: 80)")
    parser.add_argument("--cell-width", type=positive_int, default=8,
                        help="pixels per terminal column (default: 8)")
    parser.add_argument("--cell-height", type=positive_int, default=16,
                        help="pixels per terminal row (default: 16)")
    parser.add_argument("--colors", type=positive_int, default=256,
                        help="palette size, 1 to 256 (default: 256)")
    parser.add_argument("-t", "--text", help="UTF-8 welcome text placed to the right")
    parser.add_argument("--gap", type=int, default=3,
                        help="columns between image and text (default: 3)")
    parser.add_argument("--valign", choices=["top", "center", "bottom"], default="center",
                        help="vertical placement of the shorter block (default: center)")
    parser.add_argument("-o", "--output", help="output file (default: stdout)")
    args = parser.parse_args()
    if args.colors > 256:
        parser.error("colors must be between 1 and 256")
    if args.gap < 0:
        parser.error("gap must not be negative")
    try:
        image, alpha = prepare_image(args.image, args.width, args.cell_width, args.colors)
        text = read_text(args.text) if args.text else []
        sixel = encode_sixel(image, alpha)
        output = compose(sixel, math.ceil(image.height / args.cell_height), text,
                         args.width, args.gap, args.valign)
        # Write actual control bytes; preserve CR/LF on all platforms.
        if args.output:
            Path(args.output).write_bytes(output.encode("utf-8"))
        else:
            sys.stdout.buffer.write(output.encode("utf-8"))
    except (OSError, ValueError) as error:
        parser.exit(1, f"Could not generate sixel banner: {error}\n")


if __name__ == "__main__":
    main()
