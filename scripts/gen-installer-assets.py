#!/usr/bin/env python3
"""Generate the installer branding assets (NSIS bitmaps + DMG background).

Windows uses the app icon's dark/gold artwork, with one mark and no baked-in text.
The DMG keeps its wordmark on an offwhite canvas. Normal mode writes the checked-in
binaries under crates/smabar/installer/. It needs rsvg-convert (librsvg2-bin) and ImageMagick
(magick or convert), so regeneration is author-local — the same split
gen-kit-classes.py uses for the format48 harvest.

--check is offline and stdlib-only so `just check` can run it anywhere: it validates
the sidebar SVG and checked-in files (BMP: uncompressed 24-bit BITMAPINFOHEADER at the
exact NSIS dimensions; PNG: 8-bit RGB IHDR at the DMG window size). A byte compare is
impossible because librsvg output is not byte-stable across versions.

Windows inherits its mark, gradients and accent from crates/smabar/icons/app-icon.svg;
the constants below only control the DMG palette and installer layouts.
"""

from __future__ import annotations

import argparse
import shutil
import struct
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# DMG palette; Windows inherits the app icon artwork.
BACKGROUND = "#F6F4EF"  # offwhite canvas
INK = "#1C1C1C"  # wordmark and glyph
ACCENT = "#F1BF10"  # vivid yellow strip

LOGO_SVG = ROOT / "shell/src/assets/smabar-logo.svg"  # wordmark, fill="#fff"
APP_ICON_SVG = ROOT / "crates/smabar/icons/app-icon.svg"

OUTPUT_DIR = ROOT / "crates/smabar/installer"
HEADER = OUTPUT_DIR / "header.bmp"  # NSIS page header (150x57, shown beside the title)
SIDEBAR = OUTPUT_DIR / "sidebar.bmp"  # NSIS welcome/finish band (164x314)
DMG_BACKGROUND = OUTPUT_DIR / "dmg-background.png"  # macOS drag-to-install window

HEADER_SIZE = (150, 57)
SIDEBAR_SIZE = (164, 314)
DMG_SIZE = (660, 400)
MSIX_DIR = ROOT / "crates/smabar/msix/Assets"
MSIX_ASSETS = {
    f"{name}.scale-{scale}.png": (size * scale + 99) // 100
    for name, size in (
        ("Square44x44Logo", 44),
        ("Square150x150Logo", 150),
        ("StoreLogo", 50),
    )
    for scale in (100, 125, 150, 200, 400)
}
MSIX_ASSETS.update(
    {
        f"Square44x44Logo.targetsize-{size}{variant}.png": size
        for size in (16, 24, 32, 48, 256)
        for variant in ("", "_altform-unplated", "_altform-lightunplated")
    }
)
ACCENT_STRIP_PX = 8

# Layout (offsets from the top canvas edge, elements horizontally centered).
HEADER_ICON_SIZE = 44
SIDEBAR_GLYPH_WIDTH = 108
SIDEBAR_GLYPH_HEIGHT = 132
SIDEBAR_GLYPH_TOP = 68
DMG_WORDMARK_WIDTH = 320
DMG_WORDMARK_TOP = 56


def fail(message: str) -> None:
    print(f"gen-installer-assets: {message}", file=sys.stderr)


def recolor(svg: Path, tmp_dir: Path) -> Path:
    """Copy the SVG with its single fill="#fff" swapped for INK."""
    text = svg.read_text(encoding="utf-8")
    if text.count('fill="#fff"') != 1:
        raise ValueError(f'{svg}: expected exactly one fill="#fff" to recolor')
    out = tmp_dir / svg.name
    out.write_text(text.replace('fill="#fff"', f'fill="{INK}"'), encoding="utf-8")
    return out


def run(cmd: list[str]) -> None:
    result = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)}\n{result.stderr.strip()}")


def rasterize(
    rsvg: str, svg: Path, out: Path, *, width: int = 0, height: int = 0
) -> None:
    size = ["-w", str(width)] if width else ["-h", str(height)]
    run([rsvg, *size, "-o", str(out), str(svg)])


def compose_header(im: str, icon: Path) -> None:
    w, h = HEADER_SIZE
    run(
        [
            im,
            "-size",
            f"{w}x{h}",
            "xc:#fff",  # NSIS Modern UI's header background, including RTL layouts.
            str(icon),
            "-gravity",
            "center",
            "-composite",
            "-alpha",
            "off",
            "-type",
            "TrueColor",
            "-compress",
            "None",
            f"BMP3:{HEADER}",
        ]
    )


def sidebar_svg() -> str:
    """Reframe the app artwork; the full wordmark contains an English slogan."""
    source = ET.parse(APP_ICON_SVG).getroot()
    ns = {"svg": "http://www.w3.org/2000/svg"}
    definitions = source.find("svg:defs", ns)
    mark = source.find("svg:svg", ns)
    if definitions is None or mark is None or not mark.get("fill"):
        raise ValueError(
            f"{APP_ICON_SVG}: expected gradient definitions and a filled SVG mark"
        )
    if any(definitions.find(f"*[@id='{name}']") is None for name in ("ground", "halo")):
        raise ValueError(f"{APP_ICON_SVG}: expected the ground and halo gradients")
    accent = mark.get("fill")
    w, h = SIDEBAR_SIZE
    mark.attrib.update(
        x=str((w - SIDEBAR_GLYPH_WIDTH) // 2),
        y=str(SIDEBAR_GLYPH_TOP),
        width=str(SIDEBAR_GLYPH_WIDTH),
        height=str(SIDEBAR_GLYPH_HEIGHT),
    )
    # The small dock silhouette refers to the product without another logo or slogan.
    return f"""<svg xmlns="http://www.w3.org/2000/svg"
width="{w}" height="{h}" viewBox="0 0 {w} {h}">
{ET.tostring(definitions, encoding="unicode")}
<rect width="{w}" height="{h}" fill="url(#ground)"/>
<ellipse cx="82" cy="134" rx="112" ry="150" fill="url(#halo)"/>
{ET.tostring(mark, encoding="unicode")}
<g stroke="#fff" stroke-opacity="0.1">
  <rect x="24.5" y="258.5" width="115" height="27" rx="13.5" fill="#fff" fill-opacity="0.04"/>
</g>
<g fill="#fff" fill-opacity="0.22">
  <circle cx="42" cy="272" r="5"/>
  <rect x="55" y="266" width="1" height="12" rx="0.5"/>
  <rect x="65" y="267" width="12" height="10" rx="3"/>
  <rect x="85" y="267" width="12" height="10" rx="3"/>
</g>
<rect x="105" y="267" width="16" height="10" rx="3" fill="{accent}"/>
</svg>
"""


def compose_sidebar(im: str, image: Path) -> None:
    run(
        [
            im,
            str(image),
            "-alpha",
            "off",
            "-type",
            "TrueColor",
            "-compress",
            "None",
            f"BMP3:{SIDEBAR}",
        ]
    )


def compose_dmg(im: str, wordmark: Path) -> None:
    w, h = DMG_SIZE
    strip_top = h - ACCENT_STRIP_PX
    run(
        [
            im,
            "-size",
            f"{w}x{h}",
            f"xc:{BACKGROUND}",
            str(wordmark),
            "-gravity",
            "north",
            "-geometry",
            f"+0+{DMG_WORDMARK_TOP}",
            "-composite",
            "-fill",
            ACCENT,
            "-draw",
            f"rectangle 0,{strip_top} {w - 1},{h - 1}",
            f"PNG24:{DMG_BACKGROUND}",
        ]
    )


def check_bmp(path: Path, expected: tuple[int, int]) -> str | None:
    """Classic NSIS-safe bitmap: BITMAPINFOHEADER, 24-bit, uncompressed."""
    if not path.is_file():
        return "missing — run scripts/gen-installer-assets.py to generate it"
    data = path.read_bytes()
    if len(data) < 54 or data[:2] != b"BM":
        return "not a BMP file"
    dib_size = struct.unpack_from("<I", data, 14)[0]
    width = struct.unpack_from("<i", data, 18)[0]
    height = abs(struct.unpack_from("<i", data, 22)[0])
    planes = struct.unpack_from("<H", data, 26)[0]
    bpp = struct.unpack_from("<H", data, 28)[0]
    compression = struct.unpack_from("<I", data, 30)[0]
    if dib_size != 40:
        return f"DIB header size {dib_size}, need 40 (BITMAPINFOHEADER) for NSIS"
    if (width, height) != expected:
        return f"{width}x{height}, expected {expected[0]}x{expected[1]}"
    if planes != 1 or bpp != 24 or compression != 0:
        return f"planes={planes} bpp={bpp} compression={compression}, need 1/24/0"
    return None


def check_png(path: Path, expected: tuple[int, int], color: int = 2) -> str | None:
    if not path.is_file():
        return "missing — run scripts/gen-installer-assets.py to generate it"
    data = path.read_bytes()
    if len(data) < 33 or data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        return "not a PNG file"
    width, height = struct.unpack_from(">II", data, 16)
    bit_depth, color_type = data[24], data[25]
    if (width, height) != expected:
        return f"{width}x{height}, expected {expected[0]}x{expected[1]}"
    if bit_depth != 8 or color_type != color:
        return f"bit depth {bit_depth} / color type {color_type}, need 8-bit color type {color}"
    return None


def check() -> int:
    ET.fromstring(
        sidebar_svg()
    )  # Catch incompatible edits to the shared app artwork in CI.
    errors = 0
    for path, expected, validator in (
        (HEADER, HEADER_SIZE, check_bmp),
        (SIDEBAR, SIDEBAR_SIZE, check_bmp),
        (DMG_BACKGROUND, DMG_SIZE, check_png),
    ):
        problem = validator(path, expected)
        if problem:
            fail(f"{path.relative_to(ROOT)}: {problem}")
            errors += 1
    for name, size in MSIX_ASSETS.items():
        problem = check_png(MSIX_DIR / name, (size, size), color=6)
        if problem:
            fail(f"MSIX {name}: {problem}")
            errors += 1
    if errors:
        return 1
    print("installer assets OK (header.bmp, sidebar.bmp, dmg-background.png)")
    return 0


def generate() -> int:
    rsvg = shutil.which("rsvg-convert")
    if rsvg is None:
        fail(
            "rsvg-convert not found — install it (Debian/Ubuntu: "
            "sudo apt-get install librsvg2-bin). Regeneration is author-local; "
            "CI only runs --check."
        )
        return 2
    im = shutil.which("magick") or shutil.which("convert")
    if im is None:
        fail(
            "ImageMagick not found — install it (Debian/Ubuntu: "
            "sudo apt-get install imagemagick). Regeneration is author-local; "
            "CI only runs --check."
        )
        return 2

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    MSIX_DIR.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp_name:
        tmp = Path(tmp_name)
        wordmark_svg = recolor(LOGO_SVG, tmp)
        sidebar_source = tmp / "sidebar.svg"
        sidebar_source.write_text(sidebar_svg(), encoding="utf-8")

        header_icon = tmp / "header-icon.png"
        sidebar = tmp / "sidebar.png"
        dmg_wordmark = tmp / "dmg-wordmark.png"
        rasterize(rsvg, APP_ICON_SVG, header_icon, width=HEADER_ICON_SIZE)
        rasterize(rsvg, sidebar_source, sidebar, width=SIDEBAR_SIZE[0])
        rasterize(rsvg, wordmark_svg, dmg_wordmark, width=DMG_WORDMARK_WIDTH)

        compose_header(im, header_icon)
        compose_sidebar(im, sidebar)
        compose_dmg(im, dmg_wordmark)
        for name, size in MSIX_ASSETS.items():
            icon = tmp / "msix-icon.png"
            rasterize(rsvg, APP_ICON_SVG, icon, width=size)
            run([im, str(icon), "-depth", "8", f"PNG32:{MSIX_DIR / name}"])

    if check() != 0:
        fail("generated assets failed their own structural check")
        return 1
    print(
        f"wrote 3 installer assets to {OUTPUT_DIR.relative_to(ROOT)} "
        f"and {len(MSIX_ASSETS)} MSIX assets to {MSIX_DIR.relative_to(ROOT)}"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="check the sidebar SVG and asset formats, dimensions and bit depth offline",
    )
    args = parser.parse_args()
    try:
        return check() if args.check else generate()
    except (OSError, ET.ParseError, ValueError, RuntimeError) as error:
        fail(str(error))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
