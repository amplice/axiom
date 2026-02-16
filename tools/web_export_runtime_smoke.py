#!/usr/bin/env python3
"""Runtime smoke for exported web bundle.

Serves export directory over HTTP and checks that the page reaches "Running".
Requires: `pip install playwright` and `python -m playwright install chromium`.
"""

from __future__ import annotations

import argparse
import functools
import http.server
import json
import pathlib
import socketserver
import threading
import time


def read_manifest(export_dir: pathlib.Path) -> dict:
    manifest_path = export_dir / "manifest.json"
    if not manifest_path.exists():
        raise RuntimeError(f"manifest missing: {manifest_path}")
    return json.loads(manifest_path.read_text(encoding="utf-8"))


def run_browser_smoke(url: str, timeout_s: float) -> None:
    try:
        from playwright.sync_api import sync_playwright
    except Exception as exc:  # pragma: no cover
        raise RuntimeError(
            "Playwright is not installed. Run: python -m pip install playwright"
        ) from exc

    deadline = time.time() + timeout_s
    page_errors: list[str] = []
    console_errors: list[str] = []
    with sync_playwright() as p:
        browser = p.chromium.launch(
            headless=True,
            args=[
                "--enable-webgl",
                "--ignore-gpu-blocklist",
                "--use-angle=swiftshader",
                "--disable-webgpu",
            ],
        )
        try:
            page = browser.new_page()
            page.on("pageerror", lambda exc: page_errors.append(str(exc)))
            page.on(
                "console",
                lambda msg: console_errors.append(msg.text)
                if msg.type == "error"
                else None,
            )
            page.goto(url, wait_until="domcontentloaded")
            status = page.locator("#status")
            while time.time() < deadline:
                text = (status.text_content() or "").strip()
                if text == "Running":
                    return
                if "Build incomplete" in text or "Failed" in text:
                    detail = ""
                    if page_errors or console_errors:
                        detail = (
                            f" page_errors={page_errors[:3]} console_errors={console_errors[:3]}"
                        )
                    raise RuntimeError(f"export status reported failure: {text}.{detail}")
                time.sleep(0.25)
            text = (status.text_content() or "").strip()
            detail = ""
            if page_errors or console_errors:
                detail = (
                    f" page_errors={page_errors[:3]} console_errors={console_errors[:3]}"
                )
            raise RuntimeError(
                f"timed out waiting for Running status; last status={text!r}.{detail}"
            )
        finally:
            browser.close()


class ReusableTCPServer(socketserver.TCPServer):
    allow_reuse_address = True


def main() -> int:
    parser = argparse.ArgumentParser(description="Web export runtime smoke")
    parser.add_argument(
        "export_dir",
        type=pathlib.Path,
        nargs="?",
        default=pathlib.Path("export/web"),
        help="Path to web export directory (default: export/web)",
    )
    parser.add_argument("--port", type=int, default=8765, help="Local server port")
    parser.add_argument(
        "--timeout",
        type=float,
        default=45.0,
        help="Max seconds to wait for Running state",
    )
    args = parser.parse_args()

    export_dir = args.export_dir.resolve()
    if not export_dir.exists():
        raise RuntimeError(f"export directory missing: {export_dir}")

    manifest = read_manifest(export_dir)
    if manifest.get("ready") is not True:
        raise RuntimeError(
            "manifest ready != true; run export with wasm-bindgen available first"
        )
    warnings = manifest.get("warnings") or []
    if warnings:
        print("[web-export-smoke] manifest warnings:")
        for item in warnings:
            print(f"  - {item}")
    stripped = manifest.get("stripped_scripts") or []
    transpiled = manifest.get("transpiled_scripts") or []
    if stripped:
        print(
            f"[web-export-smoke] stripped scripts ({len(stripped)}): {', '.join(stripped[:8])}"
        )
    if transpiled:
        print(
            f"[web-export-smoke] transpiled scripts ({len(transpiled)}): {', '.join(transpiled[:8])}"
        )

    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(export_dir))
    server = ReusableTCPServer(("127.0.0.1", args.port), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        run_browser_smoke(f"http://127.0.0.1:{args.port}/index.html", args.timeout)
    finally:
        server.shutdown()
        server.server_close()

    print("Web export runtime smoke passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
