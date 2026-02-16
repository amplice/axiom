#!/usr/bin/env python3
"""Fighting arena demo."""

import argparse
import sys

from demo_client import create_client, managed_engine, reset_entities


def run_demo(client):
    post = client.post
    get = client.get

    print("=== Fighting Game Arena Demo ===\n")

    reset_info = reset_entities(client, delete_players=True)
    print(f"0. Reset entities (players removed: {reset_info['players_removed']})")

    print("1. Setting up fighting game physics...")
    result = post(
        "/config",
        {
            "gravity": {"x": 0, "y": -980},
            "move_speed": 250,
            "tile_size": 16,
            "jump_velocity": 450,
            "fall_multiplier": 1.8,
            "coyote_frames": 3,
            "jump_buffer_frames": 3,
        },
    )
    print(f"   Config set: {result['ok']}")

    print("2. Generating fighting arena...")
    result = post(
        "/generate",
        {
            "template": "fighting_arena",
            "difficulty": 0.5,
            "seed": 42,
            "width": 25,
            "height": 12,
            "constraints": ["reachable", "bounds_check", "has_ground"],
        },
    )
    gen = result["data"]
    print(f"   Arena: {gen['tilemap']['width']}x{gen['tilemap']['height']}")
    print(f"   P1 spawn: {gen['player_spawn']}")
    print(f"   P2 position: {gen['goal']}")
    print(f"   Platforms: {gen['difficulty_metrics']['precision_sections']}")
    print(f"   Validation: {gen['validation']}")

    print("3. Loading fighting arena...")
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

    print("4. Spawning fighters...")
    p1 = post(
        "/entities/preset",
        {
            "preset": "platformer_player",
            "x": gen["player_spawn"][0],
            "y": gen["player_spawn"][1],
        },
    )
    print(f"   P1 spawned: id={p1['data']['id']}")
    p2 = post(
        "/entities",
        {
            "x": gen["goal"][0] * 16 + 8,
            "y": gen["goal"][1] * 16 + 8,
            "components": [
                {"type": "collider", "width": 12, "height": 14},
                {"type": "gravity_body"},
                {
                    "type": "horizontal_mover",
                    "speed": 250,
                    "left_action": "left",
                    "right_action": "right",
                },
                {
                    "type": "jumper",
                    "velocity": 450,
                    "action": "jump",
                    "fall_multiplier": 1.8,
                    "variable_height": True,
                    "coyote_frames": 3,
                    "buffer_frames": 3,
                },
            ],
        },
    )
    print(f"   P2 spawned: id={p2['data']['id']}")

    print("5. Simulating P1 rushing toward P2...")
    sim_result = post(
        "/simulate",
        {
            "inputs": [
                {"frame": 0, "action": "right", "duration": 60},
                {"frame": 30, "action": "jump", "duration": 15},
                {"frame": 60, "action": "right", "duration": 30},
                {"frame": 75, "action": "jump", "duration": 10},
            ],
            "max_frames": 120,
            "record_interval": 5,
        },
    )
    sim = sim_result["data"]
    print(f"   Outcome: {sim['outcome']}")
    print(f"   Frames: {sim['frames_elapsed']}")
    print(f"   Events: {[event['type'] for event in sim['events']]}")
    if sim["trace"]:
        last = sim["trace"][-1]
        print(f"   P1 final pos: ({last['x']:.1f}, {last['y']:.1f})")

    print("6. Measuring jump feel for fighting game tuning...")
    feel = get("/feel/jump")
    profile = feel["data"]
    print(f"   Rise frames: {profile['rise_frames']}")
    print(f"   Fall frames: {profile['fall_frames']}")
    print(f"   Max height: {profile['max_height_tiles']:.1f} tiles")
    print(f"   Horizontal distance: {profile['horizontal_distance_tiles']:.1f} tiles")
    print(f"   Variable height: {profile['variable_height']}")

    print("7. Listing all entities...")
    entities = get("/entities")
    entity_items = entities["data"]
    max_rows = 20
    print(f"   Total entities: {len(entity_items)}")
    for entity in entity_items[:max_rows]:
        print(
            f"   - id={entity['id']}, components={entity['components']}, "
            f"pos=({entity['x']:.0f},{entity['y']:.0f})"
        )
    if len(entity_items) > max_rows:
        print(f"   ... and {len(entity_items) - max_rows} more")

    print("\n=== Fighting Game Demo Complete ===")
    return 0


def main():
    parser = argparse.ArgumentParser(description="Axiom fighting arena demo")
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
