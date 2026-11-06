"""Repeatable segmentation of the original JPEG using ImageMagick (no dependencies).

The source is authoritative. Re-run: python3 assets/build-assets.py.
"""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "Logo-Kit.jpeg"
# x, y, width, height: labels excluded; coordinates in original 1792 x 2400 JPEG.
REGIONS = {
    "01-primary-horizontal-lockup": (190, 235, 1420, 335),
    "02-large-standalone-emblem": (490, 820, 810, 770),
    "03-small-application-icons": (145, 1960, 570, 280),
    "04-monochrome-lockup": (920, 1985, 740, 225),
    "05-application-tile": (145, 1960, 270, 280),
    "06-small-emblem": (470, 1970, 245, 260),
}


def convert(*args):
    subprocess.run(["convert", *map(str, args)], check=True)


def build():
    records = []
    paths = [str(ROOT / "segmented" / f"{name}-{variant}.png") for name in REGIONS for variant in ("source", "transparent", "transparent-padded", "whitespace-transparent")]
    subprocess.run(["bank", *paths, "-p"], check=True, stdout=subprocess.DEVNULL)
    for name, (x, y, w, h) in REGIONS.items():
        base = ROOT / "segmented" / name
        source = Path(f"{base}-source.png")
        tight = Path(f"{base}-transparent.png")
        padded = Path(f"{base}-transparent-padded.png")
        knockout = Path(f"{base}-whitespace-transparent.png")
        convert(SOURCE, "-crop", f"{w}x{h}+{x}+{y}", "+repage", source)
        # The app tile retains its navy ink: remove only the exterior connected
        # background. Other marks intentionally have transparent navy counters.
        if name in ("05-application-tile", "03-small-application-icons"):
            key = ["-fill", "none", "-draw", "matte 0,0 floodfill"]
        else:
            key = ["-transparent", "#121f2f"]
        convert(source, "-alpha", "on", "-fuzz", "18%", *key,
                "-trim", "+repage", tight)
        convert(tight, "-bordercolor", "none", "-border", "64", padded)
        # Remove cream/near-white artwork too, leaving lime and any navy ink.
        # Monochrome naturally becomes empty: document this rather than imply
        # that it is suitable for displaying that mark.
        convert(tight, "-fuzz", "18%", "-transparent", "#f3efe3", knockout)
        records.append({"name": name, "source_box_xywh": [x, y, w, h],
                        "files": [p.name for p in (source, tight, padded, knockout)]})
    manifest = {"source": SOURCE.name,
                "sha256": hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                "transparent_padding_px": 64, "regions": records}
    (ROOT / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    build()
