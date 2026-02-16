#!/usr/bin/env python3
import argparse
import pathlib
import sys

from demo_client import create_client, managed_engine


def expect_file(path_value: str, label: str) -> None:
    path = pathlib.Path(path_value)
    if not path.exists():
        raise RuntimeError(f"{label} missing: {path}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Axiom export smoke")
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
    parser.add_argument(
        "--require-ready",
        action="store_true",
        help="Fail if web export is not wasm-bindgen ready.",
    )
    args = parser.parse_args()

    client = create_client(timeout=1800.0)
    with managed_engine(
        client,
        start_engine=args.start_engine,
        startup_timeout=args.startup_timeout,
    ):
        print("1. Exporting web bundle (debug profile)...")
        web_resp = client.post(
            "/export/web",
            {
                "title": "AxiomExportSmokeWeb",
                "width": 960,
                "height": 540,
                "levels": "all",
                "embed_assets": True,
                "release": False,
            },
        )
        if not web_resp.get("ok"):
            raise RuntimeError(f"/export/web failed: {web_resp.get('error', 'unknown error')}")
        web = web_resp.get("data") or {}
        web_artifacts = web.get("artifacts", {})
        expect_file(web_artifacts.get("index_html", ""), "web index.html")
        expect_file(web_artifacts.get("game_data_json", ""), "web game_data.json")
        expect_file(web_artifacts.get("raw_wasm", ""), "web wasm")
        if args.require_ready and web.get("ready") is not True:
            raise RuntimeError(
                "web export not ready=true (install wasm-bindgen-cli and re-run export)"
            )
        print(
            f"   web ready={web.get('ready')} root={web_artifacts.get('root', '<unknown>')}"
        )
        stripped = web.get("stripped_scripts") or []
        transpiled = web.get("transpiled_scripts") or []
        if stripped:
            print(f"   stripped scripts ({len(stripped)}): {', '.join(stripped[:8])}")
        if transpiled:
            print(f"   transpiled scripts ({len(transpiled)}): {', '.join(transpiled[:8])}")
        warnings = web.get("warnings") or []
        if warnings:
            print("   web warnings:")
            for item in warnings:
                print(f"     - {item}")

        print("2. Exporting desktop bundle (debug profile)...")
        desktop_resp = client.post(
            "/export/desktop",
            {
                "title": "AxiomExportSmokeDesktop",
                "release": False,
            },
        )
        if not desktop_resp.get("ok"):
            raise RuntimeError(
                f"/export/desktop failed: {desktop_resp.get('error', 'unknown error')}"
            )
        desktop = desktop_resp.get("data") or {}
        desktop_artifacts = desktop.get("artifacts", {})
        expect_file(desktop_artifacts.get("binary", ""), "desktop binary")
        expect_file(desktop_artifacts.get("game_data_json", ""), "desktop game_data.json")
        print(
            f"   desktop target={desktop.get('target', '<host>')} binary={desktop_artifacts.get('binary', '<unknown>')}"
        )

    print("Export smoke complete.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"[demo_export] ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
