#!/usr/bin/env python3
"""Template matrix smoke: load each generator template and run /solve."""

import argparse
import sys

from demo_client import create_client, managed_engine


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


def load_and_solve_template(client, template: str, difficulty: float, seed: int):
    config_overrides = {}
    if template in TOP_DOWN_TEMPLATES:
        config_overrides = {"gravity": 0.0, "jump_velocity": 0.0, "move_speed": 190.0}

    req = {
        "template": template,
        "difficulty": difficulty,
        "seed": int(seed),
        "config_overrides": config_overrides,
    }
    loaded = client.post("/game/load_level", req)
    if not loaded.get("ok"):
        return {
            "template": template,
            "ok": False,
            "error": loaded.get("error", "load failed"),
            "solved": False,
            "outcome": None,
            "frames": None,
        }

    solved = client.post("/solve", {})
    if not solved.get("ok"):
        return {
            "template": template,
            "ok": False,
            "error": solved.get("error", "solve failed"),
            "solved": False,
            "outcome": None,
            "frames": None,
        }
    sdata = solved.get("data") or {}
    sim = sdata.get("simulation") or {}
    return {
        "template": template,
        "ok": True,
        "error": None,
        "solved": bool(sdata.get("solved", False)),
        "outcome": sim.get("outcome"),
        "frames": sim.get("frames_elapsed"),
    }


def run_template_matrix(client, difficulty: float, seed: int, min_solved: int):
    print("Template solve matrix")
    print(f"  difficulty={difficulty:.2f} seed={seed}")
    print()

    rows = []
    for idx, template in enumerate(TEMPLATES, start=1):
        result = load_and_solve_template(client, template, difficulty, seed)
        rows.append(result)
        if result["ok"]:
            print(
                f"[{idx:02d}/{len(TEMPLATES)}] {template:<18} "
                f"solved={str(result['solved']).lower():<5} "
                f"outcome={str(result['outcome']):<12} "
                f"frames={result['frames']}"
            )
        else:
            print(f"[{idx:02d}/{len(TEMPLATES)}] {template:<18} ERROR {result['error']}")

    solved_count = sum(1 for r in rows if r["ok"] and r["solved"])
    ok_count = sum(1 for r in rows if r["ok"])
    print()
    print(f"Summary: solved {solved_count}/{len(TEMPLATES)} templates ({ok_count} loaded).")
    if ok_count != len(TEMPLATES):
        return 1
    if solved_count < min_solved:
        print(f"FAIL: solved_count {solved_count} < required minimum {min_solved}")
        return 1
    return 0


def main():
    parser = argparse.ArgumentParser(description="Axiom generator template solve matrix demo")
    parser.add_argument(
        "--difficulty",
        type=float,
        default=0.5,
        help="Difficulty to use when loading each template (default: 0.5).",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Seed to use for each template (default: 42).",
    )
    parser.add_argument(
        "--min-solved",
        type=int,
        default=0,
        help="Fail if fewer than this many templates are solved (default: 0).",
    )
    parser.add_argument(
        "--start-engine",
        action="store_true",
        help="Start a temporary local headless engine for this demo run.",
    )
    parser.add_argument(
        "--startup-timeout",
        type=float,
        default=90.0,
        help="Seconds to wait for API readiness (default: 90).",
    )
    args = parser.parse_args()

    client = create_client(timeout=60.0)
    try:
        with managed_engine(
            client,
            start_engine=args.start_engine,
            startup_timeout=args.startup_timeout,
        ):
            return run_template_matrix(
                client,
                difficulty=args.difficulty,
                seed=args.seed,
                min_solved=max(0, args.min_solved),
            )
    except RuntimeError as exc:
        print(exc)
        return 2


if __name__ == "__main__":
    sys.exit(main())
