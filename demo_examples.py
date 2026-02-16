#!/usr/bin/env python3
"""Run all built-in examples through a lightweight API smoke loop."""

import argparse
import sys
from demo_client import create_client, managed_engine


def is_solver_supported(constraints):
    tags = set(constraints or [])
    return (
        "reachable" in tags
        or "completable" in tags
        or "top_down_reachable" in tags
    )


def run_examples(client):
    examples_resp = client.get("/examples")
    examples = examples_resp.get("data") or []
    if not examples:
        print("No built-in examples were returned by /examples.")
        return 1

    print(f"Found {len(examples)} built-in examples.")
    print()

    failures = 0
    for i, ex in enumerate(examples, start=1):
        name = ex.get("name", "<unknown>")
        constraints = ex.get("constraints", [])
        print(f"[{i}/{len(examples)}] {name}")

        try:
            load = client.post(f"/examples/{name}/load", {})
            if not load.get("ok"):
                print(f"  FAIL load: {load.get('error', 'unknown error')}")
                failures += 1
                continue

            validate_req = {"constraints": constraints} if constraints else {"constraints": []}
            validation = client.post("/validate", validate_req)
            valid = bool((validation.get("data") or {}).get("valid", False))

            solved = None
            if is_solver_supported(constraints):
                solve = client.post("/solve")
                solved = bool((solve.get("data") or {}).get("solved", False))

            entities = len((client.get("/entities").get("data") or []))
            state = (client.get("/game/state").get("data") or {}).get("state", "unknown")

            status = "OK" if valid else "FAIL"
            if status == "FAIL":
                failures += 1

            solved_str = "-" if solved is None else ("yes" if solved else "no")
            solve_note = ""
            if solved is False:
                solve_note = " (solver warning)"
            print(
                f"  {status} valid={str(valid).lower()} solved={solved_str} "
                f"entities={entities} state={state}{solve_note}"
            )
        except Exception as exc:
            failures += 1
            print(f"  FAIL exception: {exc}")

    print()
    passed = len(examples) - failures
    print(f"Summary: {passed}/{len(examples)} examples passed.")
    return 0 if failures == 0 else 1


def main():
    parser = argparse.ArgumentParser(description="Axiom built-in examples smoke demo")
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
            return run_examples(client)
    except RuntimeError as exc:
        print(exc)
        return 2


if __name__ == "__main__":
    sys.exit(main())
