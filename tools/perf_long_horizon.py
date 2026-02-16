#!/usr/bin/env python3
"""Long-horizon perf characterization for built-in examples.

Runs each built-in example for a sustained window, samples /perf, and writes a
summary report. Can optionally launch/stop the local headless engine.
"""

from __future__ import annotations

import argparse
import json
import math
import pathlib
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from typing import Any

ROOT = pathlib.Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from demo_client import create_client


def percentile(values: list[float], q: float) -> float:
    if not values:
        return 0.0
    if len(values) == 1:
        return values[0]
    q = max(0.0, min(1.0, q))
    ordered = sorted(values)
    rank = q * (len(ordered) - 1)
    lo = math.floor(rank)
    hi = math.ceil(rank)
    if lo == hi:
        return ordered[lo]
    w = rank - lo
    return ordered[lo] * (1.0 - w) + ordered[hi] * w


def summarize(values: list[float]) -> dict[str, float]:
    if not values:
        return {
            "min": 0.0,
            "avg": 0.0,
            "p50": 0.0,
            "p95": 0.0,
            "max": 0.0,
        }
    return {
        "min": min(values),
        "avg": statistics.fmean(values),
        "p50": percentile(values, 0.50),
        "p95": percentile(values, 0.95),
        "max": max(values),
    }


@dataclass
class Thresholds:
    min_fps_p50: float | None
    max_frame_ms_p95: float | None
    max_script_ms_p95: float | None
    max_physics_ms_p95: float | None


def collect_example_names(client, requested: str | None) -> list[str]:
    if requested:
        names = [n.strip() for n in requested.split(",") if n.strip()]
        if names:
            return sorted(set(names))
    response = client.get("/examples")
    if not response.get("ok"):
        raise RuntimeError(f"/examples failed: {response.get('error', 'unknown error')}")
    data = response.get("data") or []
    names = [str(item.get("name", "")).strip() for item in data]
    names = [name for name in names if name]
    if not names:
        raise RuntimeError("No examples returned by /examples")
    names.sort()
    return names


def sample_perf_window(client, seconds: float, interval: float) -> list[dict[str, Any]]:
    samples: list[dict[str, Any]] = []
    deadline = time.time() + max(0.1, seconds)
    while time.time() < deadline:
        perf = client.get("/perf")
        if perf.get("ok"):
            samples.append(perf.get("data") or {})
        time.sleep(max(0.05, interval))
    return samples


def get_perf_history_samples(client) -> list[dict[str, Any]]:
    response = client.get("/perf/history")
    if not response.get("ok"):
        raise RuntimeError(
            f"/perf/history failed: {response.get('error', 'unknown error')}"
        )
    data = response.get("data") or {}
    samples = data.get("samples") or []
    if not isinstance(samples, list):
        return []
    return [s for s in samples if isinstance(s, dict)]


def sample_perf_history_window(client, seconds: float, interval: float) -> list[dict[str, Any]]:
    before = get_perf_history_samples(client)
    start_seq = 0
    if before:
        start_seq = int(before[-1].get("seq", 0) or 0)

    deadline = time.time() + max(0.1, seconds)
    while time.time() < deadline:
        time.sleep(max(0.05, interval))

    after = get_perf_history_samples(client)
    return [s for s in after if int(s.get("seq", 0) or 0) > start_seq]


def evaluate_thresholds(summary: dict[str, Any], thresholds: Thresholds) -> tuple[bool, list[str]]:
    failures: list[str] = []
    fps_p50 = float(summary["fps"]["p50"])
    frame_p95 = float(summary["frame_time_ms"]["p95"])
    script_p95 = float(summary["script_time_ms"]["p95"])
    physics_p95 = float(summary["physics_time_ms"]["p95"])

    if thresholds.min_fps_p50 is not None and fps_p50 < thresholds.min_fps_p50:
        failures.append(f"fps.p50 {fps_p50:.2f} < {thresholds.min_fps_p50:.2f}")
    if (
        thresholds.max_frame_ms_p95 is not None
        and frame_p95 > thresholds.max_frame_ms_p95
    ):
        failures.append(
            f"frame_time_ms.p95 {frame_p95:.2f} > {thresholds.max_frame_ms_p95:.2f}"
        )
    if (
        thresholds.max_script_ms_p95 is not None
        and script_p95 > thresholds.max_script_ms_p95
    ):
        failures.append(
            f"script_time_ms.p95 {script_p95:.2f} > {thresholds.max_script_ms_p95:.2f}"
        )
    if (
        thresholds.max_physics_ms_p95 is not None
        and physics_p95 > thresholds.max_physics_ms_p95
    ):
        failures.append(
            f"physics_time_ms.p95 {physics_p95:.2f} > {thresholds.max_physics_ms_p95:.2f}"
        )
    return len(failures) == 0, failures


def parse_thresholds_object(raw: dict[str, Any] | None) -> Thresholds:
    raw = raw or {}

    def parse_opt_float(key: str) -> float | None:
        value = raw.get(key)
        if value is None:
            return None
        try:
            return float(value)
        except (TypeError, ValueError):
            return None

    return Thresholds(
        min_fps_p50=parse_opt_float("min_fps_p50"),
        max_frame_ms_p95=parse_opt_float("max_frame_ms_p95"),
        max_script_ms_p95=parse_opt_float("max_script_ms_p95"),
        max_physics_ms_p95=parse_opt_float("max_physics_ms_p95"),
    )


def thresholds_to_dict(thresholds: Thresholds) -> dict[str, float | None]:
    return {
        "min_fps_p50": thresholds.min_fps_p50,
        "max_frame_ms_p95": thresholds.max_frame_ms_p95,
        "max_script_ms_p95": thresholds.max_script_ms_p95,
        "max_physics_ms_p95": thresholds.max_physics_ms_p95,
    }


def thresholds_is_empty(thresholds: Thresholds) -> bool:
    return (
        thresholds.min_fps_p50 is None
        and thresholds.max_frame_ms_p95 is None
        and thresholds.max_script_ms_p95 is None
        and thresholds.max_physics_ms_p95 is None
    )


def load_thresholds_file(
    path: pathlib.Path | None,
) -> tuple[Thresholds, dict[str, Thresholds]]:
    if path is None:
        return Thresholds(None, None, None, None), {}
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise RuntimeError("Thresholds file must be a JSON object")

    default_thresholds = parse_thresholds_object(payload.get("default"))
    examples_raw = payload.get("examples")
    per_example: dict[str, Thresholds] = {}
    if isinstance(examples_raw, dict):
        for name, cfg in examples_raw.items():
            if not isinstance(name, str) or not isinstance(cfg, dict):
                continue
            per_example[name] = parse_thresholds_object(cfg)
    return default_thresholds, per_example


def parse_suggested_thresholds(
    payload: dict[str, Any],
) -> tuple[Thresholds, dict[str, Thresholds]]:
    default_thresholds = parse_thresholds_object(payload.get("default"))
    examples_raw = payload.get("examples")
    per_example: dict[str, Thresholds] = {}
    if isinstance(examples_raw, dict):
        for name, cfg in examples_raw.items():
            if not isinstance(name, str) or not isinstance(cfg, dict):
                continue
            per_example[name] = parse_thresholds_object(cfg)
    return default_thresholds, per_example


def merge_thresholds(base: Thresholds, override: Thresholds) -> Thresholds:
    return Thresholds(
        min_fps_p50=override.min_fps_p50
        if override.min_fps_p50 is not None
        else base.min_fps_p50,
        max_frame_ms_p95=override.max_frame_ms_p95
        if override.max_frame_ms_p95 is not None
        else base.max_frame_ms_p95,
        max_script_ms_p95=override.max_script_ms_p95
        if override.max_script_ms_p95 is not None
        else base.max_script_ms_p95,
        max_physics_ms_p95=override.max_physics_ms_p95
        if override.max_physics_ms_p95 is not None
        else base.max_physics_ms_p95,
    )


def blend_value(
    current: float | None,
    target: float | None,
    max_adjust_pct: float,
) -> float | None:
    if target is None:
        return current
    if current is None:
        return round(target, 2)
    if max_adjust_pct <= 0.0:
        return round(target, 2)
    if current <= 0.0:
        return round(target, 2)

    low = current * (1.0 - max_adjust_pct)
    high = current * (1.0 + max_adjust_pct)
    clamped = min(max(target, low), high)
    return round(clamped, 2)


def blend_thresholds(
    current: Thresholds,
    target: Thresholds,
    max_adjust_pct: float,
) -> Thresholds:
    return Thresholds(
        min_fps_p50=blend_value(
            current.min_fps_p50,
            target.min_fps_p50,
            max_adjust_pct,
        ),
        max_frame_ms_p95=blend_value(
            current.max_frame_ms_p95,
            target.max_frame_ms_p95,
            max_adjust_pct,
        ),
        max_script_ms_p95=blend_value(
            current.max_script_ms_p95,
            target.max_script_ms_p95,
            max_adjust_pct,
        ),
        max_physics_ms_p95=blend_value(
            current.max_physics_ms_p95,
            target.max_physics_ms_p95,
            max_adjust_pct,
        ),
    )


def build_calibrated_thresholds(
    current_default: Thresholds,
    current_examples: dict[str, Thresholds],
    suggested_default: Thresholds,
    suggested_examples: dict[str, Thresholds],
    max_adjust_pct: float,
    prune_missing_examples: bool,
) -> tuple[dict[str, Any], dict[str, int]]:
    calibrated_default = blend_thresholds(
        current_default,
        suggested_default,
        max_adjust_pct=max_adjust_pct,
    )

    names: set[str] = set(suggested_examples.keys())
    if not prune_missing_examples:
        names |= set(current_examples.keys())

    calibrated_examples: dict[str, dict[str, float | None]] = {}
    changed = 0
    added = 0
    removed = 0

    for name in sorted(names):
        current = current_examples.get(name, Thresholds(None, None, None, None))
        target = suggested_examples.get(name, Thresholds(None, None, None, None))
        calibrated = blend_thresholds(current, target, max_adjust_pct=max_adjust_pct)

        if prune_missing_examples and name not in suggested_examples:
            removed += 1
            continue
        if name not in current_examples and not thresholds_is_empty(calibrated):
            added += 1
        if thresholds_to_dict(current) != thresholds_to_dict(calibrated):
            changed += 1
        if not thresholds_is_empty(calibrated):
            calibrated_examples[name] = thresholds_to_dict(calibrated)

    payload = {
        "default": thresholds_to_dict(calibrated_default),
        "examples": calibrated_examples,
        "meta": {
            "generated_at_unix_ms": int(time.time() * 1000),
            "method": "bounded_blend(current, suggested)",
            "max_adjust_pct": round(max_adjust_pct, 4),
            "prune_missing_examples": prune_missing_examples,
        },
    }
    return payload, {
        "changed_examples": changed,
        "added_examples": added,
        "removed_examples": removed,
    }


def build_suggested_thresholds(report: dict[str, Any]) -> dict[str, Any]:
    examples = report.get("examples") or []
    valid_examples = [e for e in examples if isinstance(e, dict) and isinstance(e.get("summary"), dict)]

    def expand_max(value: float, min_value: float = 0.1) -> float:
        return round(max(min_value, value * 1.20), 2)

    default_min_fps: float | None = None
    default_max_frame: float | None = None
    default_max_script: float | None = None
    default_max_physics: float | None = None
    per_example: dict[str, dict[str, float]] = {}

    for item in valid_examples:
        name = str(item.get("name", "")).strip()
        summary = item.get("summary") or {}
        fps_p50 = float((summary.get("fps") or {}).get("p50", 0.0))
        frame_p95 = float((summary.get("frame_time_ms") or {}).get("p95", 0.0))
        script_p95 = float((summary.get("script_time_ms") or {}).get("p95", 0.0))
        physics_p95 = float((summary.get("physics_time_ms") or {}).get("p95", 0.0))

        if fps_p50 > 0.0:
            local_min_fps = round(max(1.0, fps_p50 * 0.85), 2)
            default_min_fps = local_min_fps if default_min_fps is None else min(default_min_fps, local_min_fps)
        local_max_frame = expand_max(frame_p95)
        local_max_script = expand_max(script_p95)
        local_max_physics = expand_max(physics_p95)

        default_max_frame = local_max_frame if default_max_frame is None else max(default_max_frame, local_max_frame)
        default_max_script = local_max_script if default_max_script is None else max(default_max_script, local_max_script)
        default_max_physics = local_max_physics if default_max_physics is None else max(default_max_physics, local_max_physics)

        if name:
            per_example[name] = {
                "min_fps_p50": round(max(1.0, fps_p50 * 0.90), 2) if fps_p50 > 0.0 else 1.0,
                "max_frame_ms_p95": round(max(0.1, frame_p95 * 1.10), 2),
                "max_script_ms_p95": round(max(0.1, script_p95 * 1.10), 2),
                "max_physics_ms_p95": round(max(0.1, physics_p95 * 1.10), 2),
            }

    return {
        "default": {
            "min_fps_p50": default_min_fps if default_min_fps is not None else 1.0,
            "max_frame_ms_p95": default_max_frame if default_max_frame is not None else 33.0,
            "max_script_ms_p95": default_max_script if default_max_script is not None else 25.0,
            "max_physics_ms_p95": default_max_physics if default_max_physics is not None else 25.0,
        },
        "examples": dict(sorted(per_example.items())),
        "meta": {
            "source_report_generated_at_unix_ms": report.get("generated_at_unix_ms"),
            "method": "auto-calibrated from observed p50/p95 with safety margin",
            "safety_margin": {
                "min_fps_p50": "85% of observed p50 for defaults, 90% per example",
                "max_*_p95": "120% of observed p95 for defaults, 110% per example",
            },
        },
    }


def run_profile(
    duration_per_example: float,
    warmup_seconds: float,
    sample_interval: float,
    examples_filter: str | None,
    thresholds: Thresholds,
    example_thresholds: dict[str, Thresholds],
    use_perf_history: bool,
) -> dict[str, Any]:
    client = create_client(timeout=120.0)
    if not client.wait_for_server(timeout=40.0):
        raise RuntimeError("Engine is not reachable at AXIOM_API_URL (or default :3000).")

    names = collect_example_names(client, examples_filter)
    report_examples: list[dict[str, Any]] = []
    overall_failures: list[str] = []

    for name in names:
        print(f"[perf-long] Loading example: {name}")
        load = client.post(f"/examples/{name}/load", {})
        if not load.get("ok"):
            err = f"{name}: /examples/{name}/load failed: {load.get('error', 'unknown error')}"
            print(f"[perf-long] {err}")
            overall_failures.append(err)
            continue

        if warmup_seconds > 0.0:
            time.sleep(warmup_seconds)

        source = "perf_polling"
        if use_perf_history:
            try:
                samples = sample_perf_history_window(
                    client,
                    duration_per_example,
                    sample_interval,
                )
                source = "perf_history"
            except Exception as exc:
                print(
                    f"[perf-long] {name}: /perf/history unavailable ({exc}); falling back to /perf polling"
                )
                samples = sample_perf_window(client, duration_per_example, sample_interval)
        else:
            samples = sample_perf_window(client, duration_per_example, sample_interval)
        if not samples:
            err = f"{name}: no perf samples collected"
            print(f"[perf-long] {err}")
            overall_failures.append(err)
            continue

        fps = [float(s.get("fps", 0.0)) for s in samples]
        frame_ms = [float(s.get("frame_time_ms", 0.0)) for s in samples]
        physics_ms = [float(s.get("physics_time_ms", 0.0)) for s in samples]
        script_ms = [float(s.get("script_time_ms", 0.0)) for s in samples]
        render_ms = [float(s.get("render_time_ms", 0.0)) for s in samples]
        entities = [int(s.get("entity_count", 0)) for s in samples]
        collisions = [int(s.get("collision_checks", 0)) for s in samples]
        spatial_cells = [int(s.get("spatial_hash_cells", 0)) for s in samples]

        stats = client.get("/scripts/stats")
        script_stats = stats.get("data") if stats.get("ok") else {}

        summary = {
            "sample_count": len(samples),
            "fps": summarize(fps),
            "frame_time_ms": summarize(frame_ms),
            "physics_time_ms": summarize(physics_ms),
            "script_time_ms": summarize(script_ms),
            "render_time_ms": summarize(render_ms),
            "entity_count": summarize([float(v) for v in entities]),
            "collision_checks": summarize([float(v) for v in collisions]),
            "spatial_hash_cells": summarize([float(v) for v in spatial_cells]),
            "scripts": {
                "loaded_scripts": int(script_stats.get("loaded_scripts", 0)),
                "recent_error_buffer_len": int(
                    script_stats.get("recent_error_buffer_len", 0)
                ),
                "disabled_entity_scripts": int(
                    script_stats.get("disabled_entity_scripts", 0)
                ),
                "disabled_global_scripts": int(
                    script_stats.get("disabled_global_scripts", 0)
                ),
                "dropped_events": int(script_stats.get("dropped_events", 0)),
            },
        }
        thresholds_for_example = merge_thresholds(
            thresholds, example_thresholds.get(name, Thresholds(None, None, None, None))
        )
        passed, failures = evaluate_thresholds(summary, thresholds_for_example)
        report_examples.append(
            {
                "name": name,
                "sample_source": source,
                "thresholds": {
                    "min_fps_p50": thresholds_for_example.min_fps_p50,
                    "max_frame_ms_p95": thresholds_for_example.max_frame_ms_p95,
                    "max_script_ms_p95": thresholds_for_example.max_script_ms_p95,
                    "max_physics_ms_p95": thresholds_for_example.max_physics_ms_p95,
                },
                "passed": passed,
                "failures": failures,
                "summary": summary,
            }
        )
        if passed:
            print(
                f"[perf-long] {name}: PASS "
                f"fps.p50={summary['fps']['p50']:.2f} "
                f"frame.p95={summary['frame_time_ms']['p95']:.2f}ms "
                f"script.p95={summary['script_time_ms']['p95']:.2f}ms "
                f"physics.p95={summary['physics_time_ms']['p95']:.2f}ms"
            )
        else:
            joined = "; ".join(failures)
            print(f"[perf-long] {name}: FAIL {joined}")
            overall_failures.extend(f"{name}: {item}" for item in failures)

    return {
        "generated_at_unix_ms": int(time.time() * 1000),
        "duration_per_example_s": duration_per_example,
        "warmup_seconds": warmup_seconds,
        "sample_interval_s": sample_interval,
        "use_perf_history": use_perf_history,
        "thresholds": {
            "min_fps_p50": thresholds.min_fps_p50,
            "max_frame_ms_p95": thresholds.max_frame_ms_p95,
            "max_script_ms_p95": thresholds.max_script_ms_p95,
            "max_physics_ms_p95": thresholds.max_physics_ms_p95,
        },
        "example_thresholds": {
            name: {
                "min_fps_p50": value.min_fps_p50,
                "max_frame_ms_p95": value.max_frame_ms_p95,
                "max_script_ms_p95": value.max_script_ms_p95,
                "max_physics_ms_p95": value.max_physics_ms_p95,
            }
            for name, value in sorted(example_thresholds.items())
        },
        "examples": report_examples,
        "passed": len(overall_failures) == 0,
        "failures": overall_failures,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Long-horizon perf profiling")
    parser.add_argument(
        "--duration-per-example",
        type=float,
        default=8.0,
        help="Sample duration per example in seconds (default: 8.0)",
    )
    parser.add_argument(
        "--warmup-seconds",
        type=float,
        default=2.0,
        help="Warm-up time after loading each example (default: 2.0)",
    )
    parser.add_argument(
        "--sample-interval",
        type=float,
        default=0.25,
        help="Perf polling interval in seconds (default: 0.25)",
    )
    parser.add_argument(
        "--examples",
        type=str,
        default=None,
        help="Optional comma-separated example names to profile",
    )
    parser.add_argument(
        "--output",
        type=pathlib.Path,
        default=pathlib.Path("artifacts/perf_long_horizon.json"),
        help="Output JSON report path",
    )
    parser.add_argument(
        "--write-thresholds",
        type=pathlib.Path,
        default=None,
        help="Optional output path for auto-suggested thresholds JSON",
    )
    parser.add_argument(
        "--apply-thresholds",
        type=pathlib.Path,
        default=None,
        help="Optional output path for calibrated thresholds JSON (bounded blend)",
    )
    parser.add_argument(
        "--max-threshold-adjust-pct",
        type=float,
        default=0.25,
        help="Max per-run threshold change ratio when using --apply-thresholds (default: 0.25)",
    )
    parser.add_argument(
        "--prune-missing-example-thresholds",
        action="store_true",
        help="When applying thresholds, drop example entries absent from current run",
    )
    parser.add_argument(
        "--thresholds-file",
        type=pathlib.Path,
        default=None,
        help="Optional JSON file with default/examples perf thresholds",
    )
    parser.add_argument(
        "--start-engine",
        action="store_true",
        help="Launch local engine with `cargo run -- --headless`",
    )
    parser.add_argument(
        "--no-perf-history",
        action="store_true",
        help="Disable /perf/history sampling and poll /perf directly",
    )
    parser.add_argument(
        "--min-fps-p50",
        type=float,
        default=None,
        help="Fail if fps.p50 falls below this threshold",
    )
    parser.add_argument(
        "--max-frame-ms-p95",
        type=float,
        default=None,
        help="Fail if frame_time_ms.p95 exceeds this threshold",
    )
    parser.add_argument(
        "--max-script-ms-p95",
        type=float,
        default=None,
        help="Fail if script_time_ms.p95 exceeds this threshold",
    )
    parser.add_argument(
        "--max-physics-ms-p95",
        type=float,
        default=None,
        help="Fail if physics_time_ms.p95 exceeds this threshold",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    file_defaults, file_example_thresholds = load_thresholds_file(args.thresholds_file)
    cli_thresholds = Thresholds(
        min_fps_p50=args.min_fps_p50,
        max_frame_ms_p95=args.max_frame_ms_p95,
        max_script_ms_p95=args.max_script_ms_p95,
        max_physics_ms_p95=args.max_physics_ms_p95,
    )
    thresholds = merge_thresholds(file_defaults, cli_thresholds)
    engine_proc: subprocess.Popen[str] | None = None
    try:
        if args.start_engine:
            print("[perf-long] Starting local headless engine...")
            engine_proc = subprocess.Popen(
                ["cargo", "run", "--quiet", "--", "--headless"]
            )

        report = run_profile(
            duration_per_example=max(0.5, args.duration_per_example),
            warmup_seconds=max(0.0, args.warmup_seconds),
            sample_interval=max(0.05, args.sample_interval),
            examples_filter=args.examples,
            thresholds=thresholds,
            example_thresholds=file_example_thresholds,
            use_perf_history=not args.no_perf_history,
        )

        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(f"[perf-long] Wrote report: {args.output}")
        suggested = build_suggested_thresholds(report)
        if args.write_thresholds is not None:
            args.write_thresholds.parent.mkdir(parents=True, exist_ok=True)
            args.write_thresholds.write_text(
                json.dumps(suggested, indent=2), encoding="utf-8"
            )
            print(f"[perf-long] Wrote suggested thresholds: {args.write_thresholds}")

        if args.apply_thresholds is not None:
            existing_source: pathlib.Path | None = None
            if args.apply_thresholds.exists():
                existing_source = args.apply_thresholds
            elif args.thresholds_file is not None and args.thresholds_file.exists():
                existing_source = args.thresholds_file

            existing_default, existing_examples = load_thresholds_file(existing_source)
            suggested_default, suggested_examples = parse_suggested_thresholds(suggested)
            calibrated, stats = build_calibrated_thresholds(
                current_default=existing_default,
                current_examples=existing_examples,
                suggested_default=suggested_default,
                suggested_examples=suggested_examples,
                max_adjust_pct=max(0.0, args.max_threshold_adjust_pct),
                prune_missing_examples=args.prune_missing_example_thresholds,
            )
            args.apply_thresholds.parent.mkdir(parents=True, exist_ok=True)
            args.apply_thresholds.write_text(
                json.dumps(calibrated, indent=2),
                encoding="utf-8",
            )
            print(
                "[perf-long] Wrote calibrated thresholds: "
                f"{args.apply_thresholds} "
                f"(changed={stats['changed_examples']} "
                f"added={stats['added_examples']} "
                f"removed={stats['removed_examples']})"
            )

        if report["passed"]:
            print("[perf-long] PASS")
            return 0

        print("[perf-long] FAIL")
        for item in report.get("failures", []):
            print(f"[perf-long]  - {item}")
        return 1
    finally:
        if engine_proc is not None and engine_proc.poll() is None:
            engine_proc.terminate()
            try:
                engine_proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                engine_proc.kill()


if __name__ == "__main__":
    raise SystemExit(main())
