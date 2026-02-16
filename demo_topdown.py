#!/usr/bin/env python3
"""Top-down dungeon demo."""

import argparse
import math
import sys

from demo_client import build_top_down_inputs, create_client, managed_engine, reset_entities


def run_demo(client):
    post = client.post
    get = client.get

    print("=== Top-Down Dungeon Demo ===\n")

    reset_info = reset_entities(client, delete_players=True)
    print(f"0. Reset entities (players removed: {reset_info['players_removed']})")

    print("1. Setting gravity to (0, 0) for top-down...")
    result = post(
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
    print(f"   Config set: {result['ok']}")

    print("2. Generating top-down dungeon...")
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
    print(f"   Dungeon: {gen['tilemap']['width']}x{gen['tilemap']['height']}")
    print(f"   Spawn: {gen['player_spawn']}")
    print(f"   Goal: {gen['goal']}")
    print(f"   Validation: {gen['validation']}")
    print(f"   Rooms: {gen['difficulty_metrics']['precision_sections']}")

    print("3. Loading level...")
    load_result = post(
        "/level",
        {
            "width": gen["tilemap"]["width"],
            "height": gen["tilemap"]["height"],
            "tiles": gen["tilemap"]["tiles"],
            "player_spawn": gen["player_spawn"],
            "goal": gen["goal"],
        },
    )
    print(f"   Level loaded: {load_result['ok']}")

    print("4. Simulating top-down pathfinding...")
    goal_world = [gen["goal"][0] * 16 + 8, gen["goal"][1] * 16 + 8]
    path_result = post(
        "/ai/pathfind",
        {
            "from": gen["player_spawn"],
            "to": goal_world,
            "path_type": "top_down",
        },
    )
    path_data = path_result.get("data") or {}
    path_points = path_data.get("path") or []
    if path_data.get("found") and len(path_points) > 1:
        spawn = gen["player_spawn"]
        start_center = [
            math.floor(spawn[0] / 16.0) * 16.0 + 8.0,
            math.floor(spawn[1] / 16.0) * 16.0 + 8.0,
        ]
        if path_points[0] != start_center:
            path_points = [start_center] + path_points
        inputs = build_top_down_inputs(
            path_points,
            move_speed=200.0,
            start_point=gen["player_spawn"],
        )
        print(f"   Path points: {len(path_points)}")
        print(f"   Input steps: {len(inputs)}")
    else:
        print("   Path not found, using fallback inputs")
        inputs = [
            {"frame": 0, "action": "right", "duration": 120},
            {"frame": 120, "action": "up", "duration": 120},
            {"frame": 240, "action": "right", "duration": 120},
        ]
    max_frame = max((step["frame"] + step["duration"] for step in inputs), default=0) + 120
    sim_result = post(
        "/simulate",
        {
            "inputs": inputs,
            "max_frames": max_frame,
            "record_interval": 20,
            "goal_position": goal_world,
            "goal_radius": 12.0,
        },
    )
    sim = sim_result["data"]
    print(f"   Outcome: {sim['outcome']}")
    print(f"   Frames: {sim['frames_elapsed']}")
    print(f"   Events: {len(sim['events'])}")
    if sim["trace"]:
        last = sim["trace"][-1]
        print(f"   Final pos: ({last['x']:.1f}, {last['y']:.1f})")

    print("5. Validating reachability...")
    val_result = post(
        "/validate",
        {
            "constraints": ["top_down_reachable", "bounds_check"],
        },
    )
    val = val_result["data"]
    print(f"   Valid: {val['valid']}")
    print(f"   Passed: {val['passed']}")
    if val["violations"]:
        for violation in val["violations"]:
            print(f"   Violation: {violation['constraint']}: {violation['message']}")

    print("6. Creating top-down player entity via preset...")
    entity_result = post(
        "/entities/preset",
        {
            "preset": "top_down_player",
            "x": gen["player_spawn"][0],
            "y": gen["player_spawn"][1],
        },
    )
    print(f"   Entity: {entity_result['data']}")

    print("7. Listing entities...")
    entities = get("/entities")
    entity_items = entities["data"]
    max_rows = 20
    print(f"   Count: {len(entity_items)}")
    for entity in entity_items[:max_rows]:
        print(
            f"   - id={entity['id']}, components={entity['components']}, "
            f"pos=({entity['x']:.0f},{entity['y']:.0f})"
        )
    if len(entity_items) > max_rows:
        print(f"   ... and {len(entity_items) - max_rows} more")

    print("\n=== Top-Down Demo Complete ===")
    return 0


def main():
    parser = argparse.ArgumentParser(description="Axiom top-down dungeon demo")
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
            return run_demo(client)
    except RuntimeError as exc:
        print(exc)
        return 2


if __name__ == "__main__":
    sys.exit(main())
