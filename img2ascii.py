#!/usr/bin/env python3
"""Convert an image to ASCII art at an arbitrary text resolution.

Examples:
    img2ascii.py logo.png                          # 80 columns wide, plain
    img2ascii.py logo.png -w 24 -r ' .:+#@' -g 0.6 # small banner
    img2ascii.py logo.png -w 60 -H 40              # force exact 60x40 characters
    img2ascii.py logo.png -c                       # color, 24-bit (truecolor)
    img2ascii.py logo.png --depth 256              # color, xterm-256 palette
    img2ascii.py logo.png --depth auto             # pick from $COLORTERM / $TERM
    img2ascii.py logo.png -t info.txt              # info text to the left of the art
    img2ascii.py logo.png -t info.txt --text-side right --valign top --gap 6
    img2ascii.py logo.png -c -t info.txt -f shelly # paste-ready Shelly function

Requires: Pillow  (pip install pillow)
"""

import argparse
import os
import re
import sys

from PIL import Image, ImageOps

# Dark -> bright. Characters are ordered by approximate ink density.
DEFAULT_RAMP = " .'`^\",:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$"
SIMPLE_RAMP = " .:-=+*#%@"

# Terminal cells are roughly twice as tall as they are wide.
CELL_ASPECT = 0.5

RESET = "\x1b[0m"

# Real ANSI color sequences (in the art) and the literal four-character form
# "\x1b[...m" that someone might type into a Shelly string (in the text file).
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
TEXT_ANSI_RE = re.compile(r"\x1b\[[0-9;]*m|\\x1b\[[0-9;]*m")

# Approximate RGB values of the 16 basic ANSI colors (xterm defaults).
# Real terminals vary, so 16-color output is inherently a rough match.
ANSI16 = [
    (0, 0, 0), (205, 0, 0), (0, 205, 0), (205, 205, 0),
    (0, 0, 238), (205, 0, 205), (0, 205, 205), (229, 229, 229),
    (127, 127, 127), (255, 0, 0), (0, 255, 0), (255, 255, 0),
    (92, 92, 255), (255, 0, 255), (0, 255, 255), (255, 255, 255),
]
CUBE_LEVELS = (0, 95, 135, 175, 215, 255)


def _dist(a, b):
    return sum((x - y) ** 2 for x, y in zip(a, b))


def sgr_truecolor(rgb):
    return "38;2;%d;%d;%d" % rgb


def sgr_256(rgb):
    """Nearest xterm-256 color: best of the 6x6x6 cube and the 24-step gray ramp."""
    idx = [min(range(6), key=lambda i: abs(CUBE_LEVELS[i] - c)) for c in rgb]
    cube_rgb = tuple(CUBE_LEVELS[i] for i in idx)
    cube_n = 16 + 36 * idx[0] + 6 * idx[1] + idx[2]

    avg = sum(rgb) / 3
    g = min(23, max(0, round((avg - 8) / 10)))
    gray_v = 8 + 10 * g
    gray_rgb = (gray_v, gray_v, gray_v)

    if _dist(rgb, gray_rgb) < _dist(rgb, cube_rgb):
        return "38;5;%d" % (232 + g)
    return "38;5;%d" % cube_n


def sgr_16(rgb):
    n = min(range(16), key=lambda i: _dist(rgb, ANSI16[i]))
    return str(30 + n if n < 8 else 90 + (n - 8))


SGR = {"truecolor": sgr_truecolor, "256": sgr_256, "16": sgr_16}


def detect_depth():
    """Best-effort guess at what the current terminal supports.

    Returns None (no color at all) when NO_COLOR is set or TERM is unset/dumb.
    Note this inspects the environment of whoever runs the script, so use an
    explicit --depth for reproducible builds.
    """
    env = os.environ
    if env.get("NO_COLOR"):
        return None
    if env.get("COLORTERM", "").lower() in ("truecolor", "24bit"):
        return "truecolor"
    term = env.get("TERM", "")
    if term in ("", "dumb"):
        return None
    if "256" in term:
        return "256"
    return "16"


def load_image(path):
    """Open an image, flattening any transparency onto a black background."""
    img = Image.open(path)
    if img.mode in ("RGBA", "LA") or (img.mode == "P" and "transparency" in img.info):
        img = img.convert("RGBA")
        bg = Image.new("RGBA", img.size, (0, 0, 0, 255))
        img = Image.alpha_composite(bg, img)
    return img.convert("RGB")


def target_size(img, width, height):
    """Work out the character grid size, correcting for cell aspect ratio."""
    if height is None:
        height = max(1, round(width * (img.height / img.width) * CELL_ASPECT))
    return width, height


def convert(img, width, height, ramp, invert, gamma, normalize, depth):
    """Return the art as a string. depth is None (plain) or a key of SGR."""
    cols, rows = target_size(img, width, height)
    small = img.resize((cols, rows), Image.LANCZOS)

    gray = small.convert("L")
    if normalize:
        # Stretch contrast so dark-ish colors (like deep purple) use the full ramp.
        gray = ImageOps.autocontrast(gray, cutoff=1)

    if invert:
        ramp = ramp[::-1]

    n = len(ramp) - 1
    gpx = gray.load()
    cpx = small.load()
    sgr = SGR[depth] if depth else None
    lines = []

    for y in range(rows):
        # Build cells first so trailing blanks can be trimmed before any escapes.
        cells = []
        for x in range(cols):
            v = (gpx[x, y] / 255.0) ** gamma
            ch = ramp[min(n, int(v * n + 0.5))]
            cells.append((ch, sgr(cpx[x, y]) if sgr and ch != " " else None))

        while cells and cells[-1][0] == " ":
            cells.pop()

        out, prev = [], None
        for ch, code in cells:
            # Only emit an escape when the color actually changes.
            if code is not None and code != prev:
                out.append("\x1b[%sm" % code)
                prev = code
            out.append(ch)
        if prev is not None:
            out.append(RESET)
        lines.append("".join(out))

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Layout: art + text side by side
# ---------------------------------------------------------------------------

def trim_blank_rows(lines):
    lines = list(lines)
    while lines and lines[0] == "":
        lines.pop(0)
    while lines and lines[-1] == "":
        lines.pop()
    return lines


def crop_indent(lines):
    """Remove the leading spaces common to every non-blank row, so the gap
    between text and art is measured from the art's actual left edge."""
    indents = [len(l) - len(l.lstrip(" ")) for l in lines if l]
    cut = min(indents) if indents else 0
    return [l[cut:] for l in lines]


def vis_art(s):
    return len(ANSI_RE.sub("", s))


def vis_text(s):
    return len(TEXT_ANSI_RE.sub("", s))


def read_text(path, strip_color=False):
    """Read the side-text file. With strip_color, remove SGR color sequences,
    both real ESC bytes and the typed four-character \\x1b[...m form."""
    with open(path, encoding="utf-8") as f:
        lines = f.read().split("\n")
    if strip_color:
        lines = [TEXT_ANSI_RE.sub("", l) for l in lines]
    return trim_blank_rows([l.rstrip() for l in lines])


def compose(art_lines, text_lines, side, gap, valign):
    """Place art and text next to each other.

    Returns rows; each row is a list of (kind, string) parts where kind is
    'art', 'text' or 'space', so callers can escape each kind differently.
    """
    art_lines = crop_indent(art_lines)
    art_w = max((vis_art(l) for l in art_lines), default=0)
    text_w = max((vis_text(l) for l in text_lines), default=0)
    total = max(len(art_lines), len(text_lines))

    def offset(n):
        return {"top": 0, "center": (total - n) // 2, "bottom": total - n}[valign]

    a0, t0 = offset(len(art_lines)), offset(len(text_lines))
    rows = []
    for i in range(total):
        a = art_lines[i - a0] if 0 <= i - a0 < len(art_lines) else ""
        t = text_lines[i - t0] if 0 <= i - t0 < len(text_lines) else ""

        if side == "left":
            parts = [("text", t), ("space", " " * (text_w - vis_text(t) + gap)), ("art", a)]
            if not a:
                parts = parts[:1]          # nothing to the right: no trailing blanks
        else:
            parts = [("art", a), ("space", " " * (art_w - vis_art(a) + gap)), ("text", t)]
            if not t:
                parts = parts[:1]
        rows.append(parts)
    return rows


def render_plain(rows):
    out = []
    for parts in rows:
        # Let people type \x1b in the text file and still preview the colors.
        out.append("".join(s.replace("\\x1b", "\x1b") if k == "text" else s
                           for k, s in parts))
    return "\n".join(out)


def esc_art(s):
    """Escape art for a Shelly string: \\ \" and $ (never interpolate art), ESC -> \\x1b."""
    s = s.replace("\\", "\\\\").replace('"', '\\"').replace("$", "\\$")
    return s.replace("\x1b", "\\x1b")


def esc_text(s):
    """Text is already Shelly string content (${var}, \\x1b..., \\n all pass through).
    Only a bare double quote is escaped so it can't end the string early."""
    return re.sub(r'(?<!\\)"', r'\\"', s)


def to_shelly(rows, fn_name):
    """Wrap composed rows as a Shelly function: one echo per row."""
    body = []
    for parts in rows:
        s = "".join(esc_text(v) if k == "text" else esc_art(v) if k == "art" else v
                    for k, v in parts)
        body.append('    echo "%s"' % s)
    return "fn %s()\n{\n%s\n}" % (fn_name, "\n".join(body))


def main():
    p = argparse.ArgumentParser(description="Convert an image to ASCII art.")
    p.add_argument("image", help="path to the input image")
    p.add_argument("-w", "--width", type=int, default=80, help="width in characters (default 80)")
    p.add_argument("-H", "--height", type=int, default=None,
                   help="height in characters (default: derived from aspect ratio)")
    p.add_argument("-r", "--ramp", default=None,
                   help="custom character ramp, darkest to brightest")
    p.add_argument("--simple", action="store_true", help="use a short 10-character ramp")
    p.add_argument("-i", "--invert", action="store_true", help="invert brightness mapping")
    p.add_argument("-g", "--gamma", type=float, default=1.0,
                   help="gamma; <1 brightens midtones, >1 darkens (default 1.0)")
    p.add_argument("--no-normalize", action="store_true", help="disable auto-contrast")
    p.add_argument("-c", "--color", action="store_true",
                   help="emit ANSI color (24-bit unless --depth says otherwise)")
    p.add_argument("-d", "--depth", choices=["truecolor", "256", "16", "auto", "none"],
                   help="color depth; implies --color. 'auto' checks $COLORTERM/$TERM; "
                        "'none' is explicit mono (also strips color codes from the -t text)")
    p.add_argument("-t", "--text", help="text file to place beside the art")
    p.add_argument("--text-side", choices=["left", "right"], default="left",
                   help="which side of the art the text goes on (default left)")
    p.add_argument("--gap", type=int, default=4,
                   help="spaces between text and art (default 4)")
    p.add_argument("--valign", choices=["top", "center", "bottom"], default="center",
                   help="vertical placement of the shorter block (default center)")
    p.add_argument("-f", "--format", choices=["text", "shelly"], default="text",
                   help="'shelly' wraps each row in an echo string inside a fn you can paste")
    p.add_argument("--fn-name", default="print_banner",
                   help="function name for --format shelly (default print_banner)")
    p.add_argument("-o", "--output", help="write to this file instead of stdout")
    args = p.parse_args()

    if args.width < 1 or (args.height is not None and args.height < 1):
        p.error("width and height must be positive")
    if args.gap < 0:
        p.error("gap must not be negative")

    ramp = args.ramp or (SIMPLE_RAMP if args.simple else DEFAULT_RAMP)
    if len(ramp) < 2:
        p.error("ramp needs at least 2 characters")

    depth = None
    if args.color or args.depth:
        depth = args.depth or "truecolor"
        if depth == "auto":
            depth = detect_depth()
        elif depth == "none":
            depth = None

    try:
        img = load_image(args.image)
    except OSError as e:
        sys.exit(f"Could not open image: {e}")

    art = convert(img, args.width, args.height, ramp,
                  args.invert, args.gamma, not args.no_normalize, depth)
    art_lines = art.split("\n")

    if args.text or args.format == "shelly":
        art_lines = trim_blank_rows(art_lines)

    if args.text:
        try:
            # Mono output means no color anywhere, including codes typed into the text file.
            text_lines = read_text(args.text, strip_color=depth is None)
        except OSError as e:
            sys.exit(f"Could not read text file: {e}")
        rows = compose(art_lines, text_lines, args.text_side, args.gap, args.valign)

        if (args.format == "shelly" and args.text_side == "left"
                and any("${" in l for l in text_lines)):
            sys.stderr.write(
                "warning: text is on the left and contains ${...}. Padding is computed from\n"
                "the template, not the expanded value, so the art will shift on any row\n"
                "where the expansion's length differs. Use --text-side right, or pad at\n"
                "runtime.\n")
    else:
        rows = [[("art", l)] for l in art_lines]

    out = to_shelly(rows, args.fn_name) if args.format == "shelly" else render_plain(rows)

    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(out + "\n")
    else:
        print(out)


if __name__ == "__main__":
    main()
