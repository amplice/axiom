#!/usr/bin/env python3
"""
Axiom Sprite Analyzer
Analyzes sprite sheets: bounding boxes, padding, frame sizes, color palettes.

Usage:
    python sprite_analyze.py <image_path> [--frame-width W] [--frame-height H]
    python sprite_analyze.py <image_path> --auto-detect
    python sprite_analyze.py <directory> --batch

Examples:
    python sprite_analyze.py idle.png --frame-width 96 --frame-height 96
    python sprite_analyze.py sprites/ --batch --auto-detect
"""

import sys
import os
import json
from pathlib import Path
from PIL import Image
from collections import Counter


def analyze_frame(img, x_off, y_off, fw, fh):
    """Analyze a single frame within a sprite sheet."""
    frame = img.crop((x_off, y_off, x_off + fw, y_off + fh))
    bbox = frame.getbbox()
    if not bbox:
        return {"empty": True, "bbox": None, "size": [0, 0], "center": [0, 0]}

    cw = bbox[2] - bbox[0]
    ch = bbox[3] - bbox[1]
    cx = bbox[0] + cw // 2
    cy = bbox[1] + ch // 2

    # Count unique colors (ignoring fully transparent)
    pixels = list(frame.getdata())
    opaque = [p[:3] for p in pixels if len(p) >= 4 and p[3] > 0] if frame.mode == "RGBA" else [p[:3] if isinstance(p, tuple) else (p, p, p) for p in pixels]
    color_count = len(set(opaque))

    # Calculate opacity coverage
    total = fw * fh
    if frame.mode == "RGBA":
        opaque_count = sum(1 for p in pixels if p[3] > 0)
    else:
        opaque_count = total

    return {
        "empty": False,
        "bbox": list(bbox),
        "size": [cw, ch],
        "center": [cx, cy],
        "usage_pct": [round(cw / fw * 100, 1), round(ch / fh * 100, 1)],
        "colors": color_count,
        "opacity_pct": round(opaque_count / total * 100, 1),
    }


def auto_detect_frame_size(img):
    """Try to detect frame size by finding repeating patterns in columns."""
    w, h = img.size

    # Common frame sizes to try
    candidates = []
    for size in [8, 16, 24, 32, 48, 64, 96, 128, 256]:
        if h == size and w % size == 0:
            candidates.append((size, size, w // size, 1))
        if w == size and h % size == 0:
            candidates.append((size, size, 1, h // size))

    # Also try h as frame height, detect columns
    if h > 0:
        for fw in range(8, w + 1):
            if w % fw == 0 and fw == h:
                cols = w // fw
                candidates.append((fw, h, cols, 1))
                break

    # Try square frames matching height
    if w % h == 0:
        candidates.append((h, h, w // h, 1))

    # Deduplicate
    seen = set()
    unique = []
    for c in candidates:
        key = (c[0], c[1])
        if key not in seen:
            seen.add(key)
            unique.append(c)

    return unique


def analyze_sheet(path, frame_width=None, frame_height=None, auto=False):
    """Analyze a sprite sheet."""
    img = Image.open(path)
    w, h = img.size

    result = {
        "file": str(path),
        "sheet_size": [w, h],
        "mode": img.mode,
    }

    # Determine frame dimensions
    if frame_width and frame_height:
        fw, fh = frame_width, frame_height
    elif auto:
        candidates = auto_detect_frame_size(img)
        if candidates:
            fw, fh = candidates[0][0], candidates[0][1]
            result["auto_detected"] = {"frame_size": [fw, fh], "alternatives": [[c[0], c[1]] for c in candidates]}
        else:
            fw, fh = w, h
            result["auto_detected"] = {"frame_size": [fw, fh], "note": "no pattern found, treating as single frame"}
    else:
        fw, fh = w, h

    cols = w // fw
    rows = h // fh
    total_frames = cols * rows

    result["frame_size"] = [fw, fh]
    result["grid"] = [cols, rows]
    result["total_frames"] = total_frames

    # Analyze each frame
    frames = []
    min_x, min_y = fw, fh
    max_x, max_y = 0, 0
    empty_count = 0

    for row in range(rows):
        for col in range(cols):
            x_off = col * fw
            y_off = row * fh
            frame_info = analyze_frame(img, x_off, y_off, fw, fh)
            frame_info["index"] = row * cols + col
            frames.append(frame_info)

            if frame_info["empty"]:
                empty_count += 1
            else:
                bb = frame_info["bbox"]
                min_x = min(min_x, bb[0])
                min_y = min(min_y, bb[1])
                max_x = max(max_x, bb[2])
                max_y = max(max_y, bb[3])

    # Aggregate stats
    non_empty = [f for f in frames if not f["empty"]]
    if non_empty:
        avg_w = sum(f["size"][0] for f in non_empty) / len(non_empty)
        avg_h = sum(f["size"][1] for f in non_empty) / len(non_empty)
        max_content = [max_x - min_x, max_y - min_y]
        content_bounds = [min_x, min_y, max_x, max_y]
    else:
        avg_w = avg_h = 0
        max_content = [0, 0]
        content_bounds = [0, 0, 0, 0]

    result["summary"] = {
        "non_empty_frames": len(non_empty),
        "empty_frames": empty_count,
        "content_bounds": content_bounds,
        "max_content_size": max_content,
        "avg_content_size": [round(avg_w, 1), round(avg_h, 1)],
        "padding": {
            "top": min_y,
            "bottom": fh - max_y,
            "left": min_x,
            "right": fw - max_x,
        },
        "recommended_trim": [min_x, min_y, max_x, max_y],
        "wasted_space_pct": round((1 - (max_content[0] * max_content[1]) / (fw * fh)) * 100, 1) if fw * fh > 0 else 0,
    }

    result["frames"] = frames
    return result


def print_summary(result):
    """Pretty-print analysis results."""
    r = result
    s = r["summary"]
    print(f"\n{'='*60}")
    print(f"  {r['file']}")
    print(f"{'='*60}")
    print(f"  Sheet:  {r['sheet_size'][0]}x{r['sheet_size'][1]}  ({r['mode']})")
    print(f"  Frame:  {r['frame_size'][0]}x{r['frame_size'][1]}  ({r['grid'][0]} cols x {r['grid'][1]} rows = {r['total_frames']} frames)")
    print(f"  Active: {s['non_empty_frames']} frames  ({s['empty_frames']} empty)")
    print(f"")
    print(f"  Content bounds:  ({s['content_bounds'][0]},{s['content_bounds'][1]}) to ({s['content_bounds'][2]},{s['content_bounds'][3]})")
    print(f"  Max content:     {s['max_content_size'][0]}x{s['max_content_size'][1]} px")
    print(f"  Avg content:     {s['avg_content_size'][0]}x{s['avg_content_size'][1]} px")
    print(f"  Wasted space:    {s['wasted_space_pct']}%")
    print(f"")
    p = s["padding"]
    print(f"  Padding:  top={p['top']}  bottom={p['bottom']}  left={p['left']}  right={p['right']}")
    print(f"  Trim to:  [{s['recommended_trim'][0]}:{s['recommended_trim'][2]}, {s['recommended_trim'][1]}:{s['recommended_trim'][3]}]")

    if "auto_detected" in r:
        ad = r["auto_detected"]
        print(f"\n  Auto-detected frame: {ad['frame_size'][0]}x{ad['frame_size'][1]}")
        if "alternatives" in ad and len(ad["alternatives"]) > 1:
            print(f"  Alternatives: {ad['alternatives']}")
    print()


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Axiom Sprite Analyzer")
    parser.add_argument("path", help="Image file or directory")
    parser.add_argument("--frame-width", "-fw", type=int, help="Frame width in pixels")
    parser.add_argument("--frame-height", "-fh", type=int, help="Frame height in pixels")
    parser.add_argument("--auto-detect", "-a", action="store_true", help="Auto-detect frame size")
    parser.add_argument("--batch", "-b", action="store_true", help="Analyze all PNGs in directory")
    parser.add_argument("--json", "-j", action="store_true", help="Output as JSON")
    args = parser.parse_args()

    path = Path(args.path)

    if args.batch or path.is_dir():
        if not path.is_dir():
            print(f"Error: {path} is not a directory", file=sys.stderr)
            sys.exit(1)
        files = sorted(path.glob("*.png"))
        results = []
        for f in files:
            r = analyze_sheet(f, args.frame_width, args.frame_height, auto=args.auto_detect or (not args.frame_width))
            results.append(r)
            if not args.json:
                print_summary(r)
        if args.json:
            print(json.dumps(results, indent=2))
    else:
        r = analyze_sheet(path, args.frame_width, args.frame_height, auto=args.auto_detect or (not args.frame_width))
        if args.json:
            print(json.dumps(r, indent=2))
        else:
            print_summary(r)


if __name__ == "__main__":
    main()
