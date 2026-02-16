#!/usr/bin/env python3
"""Axiom full autonomous loop demo."""

import argparse
import sys

from demo_client import create_client, managed_engine, reset_entities


def print_header(text):
    print(f"\n{'=' * 60}")
    print(f"  {text}")
    print(f"{'=' * 60}")


def print_step(step, text):
    print(f"\n  [{step}] {text}")


def run_demo(client):
    get = client.get
    post = client.post

    print_header("AXIOM ENGINE - FULL AUTONOMOUS LOOP DEMO")

    reset_info = reset_entities(client, delete_players=True)
    post(
        "/config",
        {
            "gravity": {"x": 0, "y": -980},
            "tile_size": 16,
            "move_speed": 200,
            "jump_velocity": 400,
            "fall_multiplier": 1.5,
            "coyote_frames": 5,
            "jump_buffer_frames": 4,
        },
    )
    print_step(
        1,
        f"Engine API is ready; reset entities (players removed: {reset_info['players_removed']}).",
    )

    print_step(2, "Checking jump feel against Celeste profile...")
    compare = get("/feel/compare?target=celeste")["data"]
    feel = compare["current"]
    match_pct = compare.get("overall_match_pct", 0.0)

    print(f"      Jump height:   {feel['max_height_tiles']:.1f} tiles")
    print(f"      Rise frames:   {feel['rise_frames']}")
    print(f"      Fall frames:   {feel['fall_frames']}")
    print(f"      Gravity ratio: {feel['gravity_ratio']:.2f}")
    print(f"      Celeste match: {match_pct:.1f}%")

    if match_pct < 70.0:
        print("      Match too low, auto-tuning...")
        tune = post("/feel/tune?target=celeste")
        if tune.get("ok"):
            compare2 = get("/feel/compare?target=celeste")["data"]
            match_pct = compare2.get("overall_match_pct", 0.0)
            print(f"      After tuning:  {match_pct:.1f}% match")
    else:
        print("      Feel is good, no tuning needed.")

    print_step(3, "Generating and solving levels of increasing difficulty...")
    difficulties = [0.05, 0.1, 0.15, 0.2, 0.25]
    results = []

    for i, diff in enumerate(difficulties):
        seed = 10 + i * 20
        width = 30 + i * 5

        gen = post(
            "/generate",
            {
                "width": width,
                "height": 12,
                "difficulty": diff,
                "seed": seed,
                "constraints": ["completable"],
            },
        )["data"]

        metrics = gen["difficulty_metrics"]
        valid = gen["validation"]["valid"]

        level = {
            "width": gen["tilemap"]["width"],
            "height": gen["tilemap"]["height"],
            "tiles": gen["tilemap"]["tiles"],
            "player_spawn": gen["player_spawn"],
            "goal": gen["goal"],
        }
        post("/level", level)

        solve = post("/solve")["data"]
        solved = solve["solved"]
        sim = solve.get("simulation", {})
        outcome = sim.get("outcome", "no_sim")
        frames = sim.get("frames_elapsed", 0)

        result = {
            "level": i + 1,
            "difficulty": diff,
            "size": f"{width}x12",
            "jumps": metrics["required_jumps"],
            "max_gap": metrics["max_gap"],
            "spikes": metrics["spike_count"],
            "valid": valid,
            "solved": solved,
            "outcome": outcome,
            "frames": frames,
            "time_sec": frames / 60.0,
        }
        results.append(result)

        status = "SOLVED" if solved else f"FAIL ({outcome})"
        print(
            f"      Level {i + 1}: diff={diff:.2f} {width}x12 | "
            f"jumps={metrics['required_jumps']} gaps={metrics['max_gap']} spikes={metrics['spike_count']} | "
            f"{status} ({frames / 60.0:.1f}s)"
        )

    print_step(4, "Final feel metrics...")
    feel_final = get("/feel/compare?target=celeste")["data"]["current"]
    physics = get("/physics")["data"]

    print(
        f"      Physics: gravity={physics['gravity']:.0f} "
        f"jump_vel={physics['jump_velocity']:.0f} speed={physics['move_speed']:.0f}"
    )
    print(
        f"      Jump:    height={feel_final['max_height_tiles']:.1f}t "
        f"rise={feel_final['rise_frames']}f fall={feel_final['fall_frames']}f "
        f"ratio={feel_final['gravity_ratio']:.2f}"
    )

    print_header("RESULTS SUMMARY")
    total = len(results)
    solved_count = sum(1 for r in results if r["solved"])

    print(
        f"""
  Levels generated:    {total}
  All validated:       {'YES' if all(r['valid'] for r in results) else 'NO'}
  Bot solve rate:      {solved_count}/{total} ({100 * solved_count / total:.0f}%)
  Celeste feel match:  {match_pct:.1f}%

  Level Details:"""
    )

    for r in results:
        status = "SOLVED" if r["solved"] else "FAILED"
        print(
            f"    #{r['level']} diff={r['difficulty']:.2f} {r['size']} | "
            f"{r['jumps']} jumps, {r['max_gap']}-tile gaps, {r['spikes']} spikes | "
            f"{status} in {r['time_sec']:.1f}s"
        )

    print_header("DEMO COMPLETE")
    return 0


def main():
    parser = argparse.ArgumentParser(description="Axiom full autonomous loop demo")
    parser.add_argument(
        "--start-engine",
        action="store_true",
        help="Start a temporary local headless engine for this demo run.",
    )
    parser.add_argument(
        "--startup-timeout",
        type=float,
        default=120.0,
        help="Seconds to wait for API readiness (default: 120).",
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
