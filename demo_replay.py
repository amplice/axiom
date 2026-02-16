#!/usr/bin/env python3
"""Replay determinism smoke demo via /replay/* endpoints."""

import argparse
import sys
from demo_client import create_client, managed_engine


def run_replay(client):
    replay_name = "determinism_smoke"

    print("1. Loading a deterministic platformer example...")
    load = client.post(f"/examples/platformer_campaign/load", {"seed": 12345})
    if not load.get("ok"):
        print(f"   FAIL load example: {load.get('error', 'unknown error')}")
        return 1

    print("2. Starting replay recording...")
    rec = client.post("/replay/record", {"name": replay_name})
    if not rec.get("ok"):
        print(f"   FAIL replay record: {rec.get('error', 'unknown error')}")
        return 1

    print("3. Running simulation input sequence...")
    sim = client.post(
        "/simulate",
        {
            "inputs": [
                {"frame": 0, "action": "right", "duration": 100},
                {"frame": 24, "action": "jump", "duration": 10},
                {"frame": 128, "action": "right", "duration": 60},
                {"frame": 136, "action": "jump", "duration": 8},
            ],
            "max_frames": 240,
            "record_interval": 20,
        },
    )
    if not sim.get("ok"):
        print(f"   FAIL simulate: {sim.get('error', 'unknown error')}")
        return 1

    print("4. Stopping replay recording...")
    stop = client.post("/replay/stop", {"name": replay_name})
    if not stop.get("ok"):
        print(f"   FAIL replay stop: {stop.get('error', 'unknown error')}")
        return 1

    print("5. Replaying and checking mismatches...")
    play = client.post("/replay/play", {"name": replay_name})
    if not play.get("ok"):
        print(f"   FAIL replay play: {play.get('error', 'unknown error')}")
        return 1

    data = play.get("data") or {}
    mismatches = int(data.get("mismatch_count", 0))
    steps = int(data.get("steps", 0))
    print(f"   replay steps={steps} mismatches={mismatches}")

    if mismatches != 0:
        print(f"   FAIL deterministic replay mismatch indices: {data.get('mismatch_indices')}")
        return 1

    print("Replay determinism smoke passed.")
    return 0


def main():
    parser = argparse.ArgumentParser(description="Axiom replay determinism smoke")
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
            return run_replay(client)
    except RuntimeError as exc:
        print(exc)
        return 2


if __name__ == "__main__":
    sys.exit(main())
