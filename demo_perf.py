#!/usr/bin/env python3
"""Spawn a large scripted crowd and report /perf metrics."""

import argparse
import sys
import time
import urllib.error
from demo_client import create_client, managed_engine


NOOP_SCRIPT = """
function update(entity, world, dt)
    if entity.grounded then
        entity.vx = 0
    else
        entity.vx = entity.vx
    end
end
""".strip()


def run_perf(client):
    target_count = 1000
    cols = 40
    spacing = 20.0

    print("1. Setting top-down perf config...")
    cfg = client.post(
        "/config",
        {
            "gravity": {"x": 0, "y": 0},
            "tile_size": 16,
            "move_speed": 0,
            "jump_velocity": 0,
            "fall_multiplier": 1.0,
            "coyote_frames": 0,
            "jump_buffer_frames": 0,
        },
    )
    if not cfg.get("ok"):
        print(f"   FAIL config: {cfg.get('error', 'unknown error')}")
        return 1

    print("2. Uploading no-op stress script...")
    upsert = client.post(
        "/scripts",
        {"name": "stress_noop", "source": NOOP_SCRIPT, "global": False},
    )
    if not upsert.get("ok"):
        print(f"   FAIL script upload: {upsert.get('error', 'unknown error')}")
        return 1

    print(f"3. Spawning {target_count} scripted entities...")
    spawned = 0
    for i in range(target_count):
        x = 48.0 + (i % cols) * spacing
        y = 48.0 + (i // cols) * spacing
        req = {
            "x": x,
            "y": y,
            "script": "stress_noop",
            "components": [
                {"type": "collider", "width": 10, "height": 10},
                {
                    "type": "top_down_mover",
                    "speed": 0,
                    "up_action": "up",
                    "down_action": "down",
                    "left_action": "left",
                    "right_action": "right",
                },
            ],
            "tags": ["perf", "stress"],
            "is_player": False,
        }
        attempt = 0
        while True:
            try:
                result = client.post("/entities", req)
                break
            except urllib.error.HTTPError as exc:
                if exc.code == 429 and attempt < 6:
                    attempt += 1
                    time.sleep(0.02 * attempt)
                    continue
                raise
        if not result.get("ok"):
            print(f"   FAIL spawn at {i}: {result.get('error', 'unknown error')}")
            break
        spawned += 1
        if i % 20 == 0:
            time.sleep(0.005)

    print(f"   Spawned: {spawned}")
    if spawned == 0:
        return 1

    print("4. Letting the engine run for warm-up...")
    time.sleep(1.5)

    print("5. Reading /perf and /scripts/stats...")
    perf = client.get("/perf")
    history = client.get("/perf/history")
    stats = client.get("/scripts/stats")
    if not perf.get("ok"):
        print(f"   FAIL perf: {perf.get('error', 'unknown error')}")
        return 1
    if not history.get("ok"):
        print(f"   FAIL perf history: {history.get('error', 'unknown error')}")
        return 1
    if not stats.get("ok"):
        print(f"   FAIL script stats: {stats.get('error', 'unknown error')}")
        return 1

    pdata = perf.get("data") or {}
    hdata = history.get("data") or {}
    sdata = stats.get("data") or {}
    print(
        "   perf: "
        f"fps={pdata.get('fps', 0):.2f} "
        f"frame_ms={pdata.get('frame_time_ms', 0):.2f} "
        f"entities={pdata.get('entity_count', 0)} "
        f"physics_ms={pdata.get('physics_time_ms', 0):.2f} "
        f"script_ms={pdata.get('script_time_ms', 0):.2f}"
    )
    print(
        "   scripts: "
        f"loaded={sdata.get('loaded_scripts', 0)} "
        f"errors={sdata.get('recent_error_buffer_len', 0)} "
        f"dropped_events={sdata.get('dropped_events', 0)}"
    )
    print(
        "   perf_history: "
        f"samples={len(hdata.get('samples') or [])} "
        f"dropped={hdata.get('dropped_samples', 0)} "
        f"capacity={hdata.get('capacity', 0)}"
    )

    print("Perf smoke complete.")
    return 0


def main():
    parser = argparse.ArgumentParser(description="Axiom /perf smoke demo")
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
            return run_perf(client)
    except RuntimeError as exc:
        print(exc)
        return 2


if __name__ == "__main__":
    sys.exit(main())
