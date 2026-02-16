#!/usr/bin/env python3
"""RTS arena demo."""

import argparse
import math
import sys

from demo_client import build_top_down_inputs, create_client, managed_engine, reset_entities


def run_demo(client):
    post = client.post
    get = client.get

    print("=== RTS Arena Demo ===\n")

    reset_info = reset_entities(client, delete_players=True)
    print(f"0. Reset entities (players removed: {reset_info['players_removed']})")

    print("1. Setting gravity to (0, 0) for RTS...")
    result = post(
        "/config",
        {
            "gravity": {"x": 0, "y": 0},
            "move_speed": 150,
            "tile_size": 16,
            "jump_velocity": 0,
            "fall_multiplier": 1.0,
            "coyote_frames": 0,
            "jump_buffer_frames": 0,
        },
    )
    print(f"   Config set: {result['ok']}")

    print("2. Generating RTS arena...")
    result = post(
        "/generate",
        {
            "template": "rts_arena",
            "difficulty": 0.4,
            "seed": 1337,
            "width": 50,
            "height": 35,
            "constraints": ["top_down_reachable", "bounds_check"],
        },
    )
    gen = result["data"]
    print(f"   Arena: {gen['tilemap']['width']}x{gen['tilemap']['height']}")
    print(f"   Spawn (base): {gen['player_spawn']}")
    print(f"   Objective: {gen['goal']}")
    print(f"   Validation: {gen['validation']}")
    print(f"   Resource nodes: {gen['difficulty_metrics']['precision_sections']}")

    print("3. Loading arena...")
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
    print(f"   Arena loaded: {load_result['ok']}")

    print("4. Spawning RTS units...")
    unit_ids = []
    for i in range(3):
        unit = post(
            "/entities/preset",
            {
                "preset": "top_down_player",
                "x": gen["player_spawn"][0] + i * 20,
                "y": gen["player_spawn"][1],
            },
        )
        unit_ids.append(unit["data"]["id"])
        print(f"   Unit {i + 1}: id={unit['data']['id']}")

    print("5. Simulating scout movement to objective...")
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
            move_speed=150.0,
            start_point=gen["player_spawn"],
        )
        print(f"   Path points: {len(path_points)}")
        print(f"   Input steps: {len(inputs)}")
    else:
        print("   Path not found, using fallback inputs")
        inputs = [
            {"frame": 0, "action": "right", "duration": 220},
            {"frame": 220, "action": "up", "duration": 220},
        ]
    max_frame = max((step["frame"] + step["duration"] for step in inputs), default=0) + 120
    sim_result = post(
        "/simulate",
        {
            "inputs": inputs,
            "max_frames": max_frame,
            "record_interval": 20,
            "goal_position": goal_world,
            "goal_radius": 16.0,
        },
    )
    sim = sim_result["data"]
    print(f"   Outcome: {sim['outcome']}")
    print(f"   Frames: {sim['frames_elapsed']}")
    if sim["trace"]:
        last = sim["trace"][-1]
        print(f"   Scout final pos: ({last['x']:.1f}, {last['y']:.1f})")

    print("6. Validating arena traversability...")
    val_result = post(
        "/validate",
        {
            "constraints": ["top_down_reachable", "bounds_check"],
        },
    )
    val = val_result["data"]
    print(f"   Valid: {val['valid']}")
    print(f"   Passed: {val['passed']}")

    print("7. Listing all entities...")
    entities = get("/entities")
    entity_items = entities["data"]
    max_rows = 20
    print(f"   Total entities: {len(entity_items)}")
    for entity in entity_items[:max_rows]:
        print(f"   - id={entity['id']}, components={entity['components']}")
    if len(entity_items) > max_rows:
        print(f"   ... and {len(entity_items) - max_rows} more")

    print("\n=== RTS Demo Complete ===")
    return 0


def main():
    parser = argparse.ArgumentParser(description="Axiom RTS arena demo")
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
