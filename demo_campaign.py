#!/usr/bin/env python3
"""Campaign/level-pack smoke demo."""

import argparse
import sys

from demo_client import create_client, managed_engine


def run_campaign_smoke(client):
    pack_name = "demo_campaign_smoke"
    pack_req = {
        "name": pack_name,
        "levels": [
            {
                "template": "top_down_dungeon",
                "difficulty": 0.30,
                "seed": 1,
                "config_overrides": {"gravity": 0.0, "jump_velocity": 0.0, "move_speed": 185.0},
            },
            {
                "template": "puzzle_platformer",
                "difficulty": 0.25,
                "seed": 6060,
                "config_overrides": {"move_speed": 220.0},
            },
            {
                "template": "fighting_arena",
                "difficulty": 0.50,
                "seed": 4,
                "config_overrides": {"move_speed": 210.0},
            },
        ],
    }

    print("1. Defining level pack...")
    defined = client.post("/levels/pack", pack_req)
    if not defined.get("ok"):
        raise RuntimeError(f"define_level_pack failed: {defined.get('error')}")
    total = (defined.get("data") or {}).get("levels", len(pack_req["levels"]))
    print(f"   Pack: {pack_name} ({total} levels)")

    print("2. Starting campaign...")
    started = client.post(f"/levels/pack/{pack_name}/start", {})
    if not started.get("ok"):
        raise RuntimeError(f"start_level_pack failed: {started.get('error')}")
    start_data = started.get("data") or {}
    print(f"   Current level: {start_data.get('current_level')} / {start_data.get('total_levels')}")

    print("3. Solving each level and advancing...")
    while True:
        progress = client.get(f"/levels/pack/{pack_name}/progress")
        if not progress.get("ok"):
            raise RuntimeError(f"level_pack_progress failed: {progress.get('error')}")
        pdata = progress.get("data") or {}
        current = int(pdata.get("current_level", 0))
        completed = bool(pdata.get("completed", False))
        if completed:
            break

        solve = client.post("/solve", {})
        if not solve.get("ok"):
            raise RuntimeError(f"solve failed: {solve.get('error')}")
        sdata = solve.get("data") or {}
        sim = sdata.get("simulation") or {}
        solved = bool(sdata.get("solved", False))
        print(
            f"   Level {current + 1}/{total}: solved={solved} "
            f"outcome={sim.get('outcome', '-')}"
        )

        advanced = client.post(f"/levels/pack/{pack_name}/next", {})
        if not advanced.get("ok"):
            raise RuntimeError(f"next_level_pack failed: {advanced.get('error')}")
        adata = advanced.get("data") or {}
        if bool(adata.get("completed", False)):
            break

    print("4. Verifying final campaign progress...")
    final_progress = client.get(f"/levels/pack/{pack_name}/progress")
    if not final_progress.get("ok"):
        raise RuntimeError(f"level_pack_progress failed: {final_progress.get('error')}")
    fdata = final_progress.get("data") or {}
    history = fdata.get("history") or []
    print(
        f"   completed={fdata.get('completed')} "
        f"history={len(history)} total={fdata.get('total_levels')}"
    )
    if not fdata.get("completed"):
        raise RuntimeError("campaign did not reach completed=true")
    if len(history) != int(fdata.get("total_levels", total)):
        raise RuntimeError("campaign history length does not match total level count")

    print("Campaign smoke complete.")


def main():
    parser = argparse.ArgumentParser(description="Axiom level-pack campaign smoke demo")
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
            run_campaign_smoke(client)
            return 0
    except RuntimeError as exc:
        print(exc)
        return 2


if __name__ == "__main__":
    sys.exit(main())
