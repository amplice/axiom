#!/usr/bin/env python3
"""Atlas Stitcher — packs autotile slot tiles into a single atlas PNG.

Usage:
    python tools/autotile_atlas_stitcher.py --profile path/to/border_primitives_runtime.json --tiles-dir "path/to/Isometric Tiles"

Reads a runtime profile (from the slot tool or primitive teacher) and the source
tile PNGs, then produces:
  - {material}_autotile_atlas.png  (up to 20 columns x 1 row, each frame 128x256)
  - {material}_autotile_atlas.json (metadata: slot name -> atlas frame index)

Atlas layout (fixed, by convention — extended 20-slot):
  Frame 0:  fill
  Frame 1:  edge_N         Frame 2:  edge_E
  Frame 3:  edge_S         Frame 4:  edge_W
  Frame 5:  outer_NW       Frame 6:  outer_NE
  Frame 7:  outer_SE       Frame 8:  outer_SW
  Frame 9:  inner_NW       Frame 10: inner_NE
  Frame 11: inner_SE       Frame 12: inner_SW
  Frame 13: endcap_N       Frame 14: endcap_E
  Frame 15: endcap_S       Frame 16: endcap_W
  Frame 17: lane_NS        Frame 18: lane_EW
  Frame 19: full_surround

Backward compatible: 13-slot profiles produce a 13-frame atlas.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    print("Error: Pillow is required. Install with: pip install Pillow")
    raise SystemExit(1)

# Atlas slot order — classic 13 slots (always included)
ATLAS_SLOTS_13 = [
    ("fill", None),
    ("edge", "N"), ("edge", "E"), ("edge", "S"), ("edge", "W"),
    ("outside_corner", "NW"), ("outside_corner", "NE"),
    ("outside_corner", "SE"), ("outside_corner", "SW"),
    ("inside_corner", "NW"), ("inside_corner", "NE"),
    ("inside_corner", "SE"), ("inside_corner", "SW"),
]

ATLAS_SLOT_NAMES_13 = [
    "fill",
    "edge_N", "edge_E", "edge_S", "edge_W",
    "outer_NW", "outer_NE", "outer_SE", "outer_SW",
    "inner_NW", "inner_NE", "inner_SE", "inner_SW",
]

# Extended 7 slots (appended when profile has endcap/lane/full_surround)
ATLAS_SLOTS_EXT = [
    ("endcap", "N"), ("endcap", "E"), ("endcap", "S"), ("endcap", "W"),
    ("lane", "NS"), ("lane", "EW"),
    ("full_surround", None),
]

ATLAS_SLOT_NAMES_EXT = [
    "endcap_N", "endcap_E", "endcap_S", "endcap_W",
    "lane_NS", "lane_EW",
    "full_surround",
]

DIR_SET = {"N", "E", "S", "W"}


def resolve_tile_key(slots: dict, ptype: str, orient: str | None, fallback: str) -> str:
    """Look up a tile key from compiled_grid_frame_slots."""
    if ptype == "fill":
        return slots.get('fill', fallback)
    if ptype == "full_surround":
        val = slots.get('full_surround', fallback)
        # full_surround can be a string or a dict; if dict, use orient
        if isinstance(val, dict):
            return val.get(orient, fallback) if orient else fallback
        return val
    sub = slots.get(ptype) or {}
    if isinstance(sub, str):
        return sub  # e.g. full_surround stored as plain string
    return sub.get(orient, fallback)


def find_tile_png(tiles_dir: Path, tile_key: str) -> Path | None:
    """Resolve a tile key like 'G1_N' to a PNG path."""
    parts = tile_key.split("_", 1)
    if len(parts) != 2 or parts[1] not in DIR_SET:
        return None
    fam, d = parts
    fp = tiles_dir / f"Ground {fam}_{d}.png"
    if fp.exists():
        return fp
    # Also try without "Ground " prefix (generated dir uses different naming)
    fp2 = tiles_dir / f"{fam}_{d}.png"
    if fp2.exists():
        return fp2
    return None


def _profile_has_extended_slots(slots: dict) -> bool:
    """Check if the profile defines any of the 7 extended slot types."""
    for key in ('endcap', 'lane', 'full_surround'):
        if key in slots:
            return True
    return False


def stitch_atlas(
    profile_path: Path,
    tiles_dir: Path,
    output_path: Path | None = None,
    fw: int = 128,
    fh: int = 256,
) -> dict:
    """Stitch an autotile atlas from a profile + tile PNGs.

    Produces 13 frames for classic profiles, 20 frames for extended profiles
    (endcap/lane/full_surround).

    Returns a dict with 'atlas_path', 'meta_path', and 'missing' list.
    Can be called from the slot tool or from the CLI.
    """
    data = json.loads(profile_path.read_text(encoding='utf-8'))
    slots = data.get('compiled_grid_frame_slots') or {}
    material = data.get('material') or {}
    series = material.get('series', '?')
    label = material.get('label', series.lower())
    fallback_key = slots.get('fill', f'{series}1_N')

    # Determine slot layout: 13 (classic) or 20 (extended)
    extended = _profile_has_extended_slots(slots)
    if extended:
        atlas_slots = ATLAS_SLOTS_13 + ATLAS_SLOTS_EXT
        slot_names = ATLAS_SLOT_NAMES_13 + ATLAS_SLOT_NAMES_EXT
    else:
        atlas_slots = ATLAS_SLOTS_13
        slot_names = ATLAS_SLOT_NAMES_13

    print(f"Atlas Stitcher")
    print(f"  Material: {label} ({series})")
    print(f"  Tiles dir: {tiles_dir}")
    print(f"  Layout: {len(atlas_slots)} slots ({'extended' if extended else 'classic'})")

    atlas_path = output_path or (profile_path.parent / f'{label}_{series}_autotile_atlas.png')
    meta_path = atlas_path.with_suffix('.json')

    n_frames = len(atlas_slots)
    atlas = Image.new('RGBA', (fw * n_frames, fh), (0, 0, 0, 0))

    metadata = {}
    missing = []

    for i, (ptype, orient) in enumerate(atlas_slots):
        tile_key = resolve_tile_key(slots, ptype, orient, fallback_key)
        tile_png = find_tile_png(tiles_dir, tile_key)

        if tile_png is None:
            tile_png = find_tile_png(tiles_dir, fallback_key)
            if tile_png is None:
                missing.append((slot_names[i], tile_key))
                print(f"  WARNING: missing tile for slot {slot_names[i]}: {tile_key}")
                continue

        tile_img = Image.open(tile_png).convert('RGBA')
        if tile_img.size != (fw, fh):
            tile_img = tile_img.resize((fw, fh), Image.LANCZOS)

        atlas.paste(tile_img, (i * fw, 0))
        metadata[slot_names[i]] = {
            'frame_index': i,
            'tile_key': tile_key,
            'source': str(tile_png.name),
        }
        print(f"  [{i:2d}] {slot_names[i]:16s} <- {tile_key}")

    atlas_path.parent.mkdir(parents=True, exist_ok=True)
    atlas.save(str(atlas_path), 'PNG')
    print(f"  Atlas saved: {atlas_path}")
    print(f"  Dimensions: {atlas.size[0]}x{atlas.size[1]} ({n_frames} frames)")

    meta = {
        'version': 2 if extended else 1,
        'format': 'axiom_autotile_atlas',
        'material': material,
        'frame_width': fw,
        'frame_height': fh,
        'columns': n_frames,
        'rows': 1,
        'slots': metadata,
        'slot_order': slot_names,
    }
    if missing:
        meta['missing'] = [{'slot': s, 'tile_key': k} for s, k in missing]
    meta_path.write_text(json.dumps(meta, indent=2), encoding='utf-8')
    print(f"  Metadata saved: {meta_path}")

    if missing:
        print(f"  WARNING: {len(missing)} slots had missing tiles (used fallback or empty)")

    return {'atlas_path': str(atlas_path), 'meta_path': str(meta_path), 'missing': missing}


def main():
    ap = argparse.ArgumentParser(description='Autotile Atlas Stitcher')
    ap.add_argument('--profile', '-p', required=True, help='Path to border_primitives_runtime.json')
    ap.add_argument('--tiles-dir', '-d', required=True, help='Directory containing tile PNGs')
    ap.add_argument('--output', '-o', default=None, help='Output path for atlas PNG (default: auto)')
    ap.add_argument('--frame-width', type=int, default=128, help='Tile frame width (default: 128)')
    ap.add_argument('--frame-height', type=int, default=256, help='Tile frame height (default: 256)')
    args = ap.parse_args()

    profile_path = Path(args.profile).resolve()
    tiles_dir = Path(args.tiles_dir).resolve()

    if not profile_path.exists():
        print(f"Error: profile not found: {profile_path}")
        raise SystemExit(1)
    if not tiles_dir.is_dir():
        print(f"Error: tiles directory not found: {tiles_dir}")
        raise SystemExit(1)

    output = Path(args.output).resolve() if args.output else None
    stitch_atlas(profile_path, tiles_dir, output, args.frame_width, args.frame_height)


if __name__ == '__main__':
    main()
