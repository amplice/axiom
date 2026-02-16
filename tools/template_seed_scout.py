#!/usr/bin/env python3
"""Find solver-friendly seeds for each generation template."""

import argparse
import json
import pathlib
import sys


REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from demo_client import create_client, managed_engine  # noqa: E402


TEMPLATES = [
    "platformer",
    "top_down_dungeon",
    "rts_arena",
    "fighting_arena",
    "metroidvania",
    "roguelike_floor",
    "puzzle_platformer",
    "arena_waves",
    "side_scroller",
    "tower_defense_map",
    "boss_arena",
]

TOP_DOWN_TEMPLATES = {
    "top_down_dungeon",
    "rts_arena",
    "roguelike_floor",
    "arena_waves",
    "tower_defense_map",
}


def config_overrides_for_template(template: str):
    if template in TOP_DOWN_TEMPLATES:
        return {"gravity": 0.0, "jump_velocity": 0.0, "move_speed": 190.0}
    return {}


def try_seed(client, template: str, difficulty: float, seed: int):
    load = client.post(
        "/game/load_level",
        {
            "template": template,
            "difficulty": difficulty,
            "seed": int(seed),
            "config_overrides": config_overrides_for_template(template),
        },
    )
    if not load.get("ok"):
        return False, "load_error", None

    solve = client.post("/solve", {})
    if not solve.get("ok"):
        return False, "solve_error", None
    data = solve.get("data") or {}
    sim = data.get("simulation") or {}
    return bool(data.get("solved", False)), sim.get("outcome", "unknown"), sim.get("frames_elapsed")


def scout(client, templates, difficulty: float, seed_start: int, seed_end: int):
    report = {"difficulty": difficulty, "seed_range": [seed_start, seed_end], "results": {}}
    for template in templates:
        found = None
        attempts = 0
        print(f"\n[{template}]")
        for seed in range(seed_start, seed_end + 1):
            attempts += 1
            solved, outcome, frames = try_seed(client, template, difficulty, seed)
            if solved:
                found = {"seed": seed, "outcome": outcome, "frames": frames, "attempts": attempts}
                print(f"  solved at seed={seed} frames={frames}")
                break
        if not found:
            found = {"seed": None, "outcome": "not_found", "frames": None, "attempts": attempts}
            print("  no solved seed in range")
        report["results"][template] = found
    return report


def main():
    parser = argparse.ArgumentParser(description="Template seed scout for /solve")
    parser.add_argument("--difficulty", type=float, default=0.5)
    parser.add_argument("--seed-start", type=int, default=1)
    parser.add_argument("--seed-end", type=int, default=200)
    parser.add_argument(
        "--templates",
        type=str,
        default="all",
        help='Comma-separated template names or "all".',
    )
    parser.add_argument("--write-json", type=str, default="")
    parser.add_argument("--start-engine", action="store_true")
    parser.add_argument("--startup-timeout", type=float, default=90.0)
    args = parser.parse_args()

    if args.seed_end < args.seed_start:
        print("seed-end must be >= seed-start")
        return 2

    if args.templates.strip().lower() == "all":
        templates = list(TEMPLATES)
    else:
        templates = [t.strip() for t in args.templates.split(",") if t.strip()]
        unknown = [t for t in templates if t not in TEMPLATES]
        if unknown:
            print(f"Unknown template(s): {', '.join(unknown)}")
            return 2

    client = create_client(timeout=60.0)
    try:
        with managed_engine(
            client,
            start_engine=args.start_engine,
            startup_timeout=args.startup_timeout,
        ):
            report = scout(
                client,
                templates,
                difficulty=args.difficulty,
                seed_start=args.seed_start,
                seed_end=args.seed_end,
            )
    except RuntimeError as exc:
        print(exc)
        return 2

    solved_count = sum(1 for item in report["results"].values() if item["seed"] is not None)
    print(f"\nSolved templates in range: {solved_count}/{len(report['results'])}")

    if args.write_json.strip():
        out_path = pathlib.Path(args.write_json).expanduser().resolve()
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(f"Wrote {out_path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
