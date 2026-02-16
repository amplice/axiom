#!/usr/bin/env python3
"""Load a top-down dungeon into Axiom."""

import argparse
import sys

from demo_client import create_client, managed_engine, reset_entities


def run_loader(client):
    post = client.post

    post(
        "/config",
        {
            "gravity": {"x": 0, "y": 0},
            "move_speed": 200,
            "tile_size": 16,
            "jump_velocity": 0,
            "fall_multiplier": 1.0,
            "coyote_frames": 0,
            "jump_buffer_frames": 0,
        },
    )
    print("Config: zero gravity top-down")

    result = post(
        "/generate",
        {
            "template": "top_down_dungeon",
            "difficulty": 0.3,
            "seed": 777,
            "width": 30,
            "height": 25,
            "constraints": ["top_down_reachable", "bounds_check"],
        },
    )
    gen = result["data"]
    spawn = gen["player_spawn"]
    print(f"Dungeon: {gen['tilemap']['width']}x{gen['tilemap']['height']}, spawn at {spawn}")

    post(
        "/level",
        {
            "width": gen["tilemap"]["width"],
            "height": gen["tilemap"]["height"],
            "tiles": gen["tilemap"]["tiles"],
            "player_spawn": spawn,
            "goal": gen["goal"],
        },
    )
    print("Level loaded")

    reset_info = reset_entities(client, delete_players=True)
    print(f"Reset entities (players removed: {reset_info['players_removed']})")

    result = post(
        "/entities/preset",
        {
            "preset": "top_down_player",
            "x": spawn[0],
            "y": spawn[1],
        },
    )
    print(f"Spawned top-down player: {result['data']}")
    print("\nDungeon loaded! Use WASD to explore.")
    return 0


def main():
    parser = argparse.ArgumentParser(description="Load top-down dungeon utility")
    parser.add_argument(
        "--start-engine",
        action="store_true",
        help="Start a temporary local headless engine for this run.",
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
            return run_loader(client)
    except RuntimeError as exc:
        print(exc)
        return 2


if __name__ == "__main__":
    sys.exit(main())
