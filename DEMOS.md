# Axiom Demo Scripts

All demos use `demo_client.create_client()`, which reads:

- `AXIOM_API_URL` (default: `http://127.0.0.1:3000`)
- `AXIOM_API_TOKEN` (optional; sent as `Authorization: Bearer <token>`)
- `AXIOM_API_RETRIES` (default: `2` for read requests)
- `AXIOM_API_RETRY_BACKOFF_MS` (default: `150`)

Start the engine first (headless or windowed), for example:

```powershell
cargo run -- --headless
```

Then run any demo:

```powershell
python demo.py
```

Or let compatible demos start/stop a temporary engine automatically:

```powershell
python demo_examples.py --start-engine
python demo_replay.py --start-engine
python demo_perf.py --start-engine
python demo_export.py --start-engine
python demo_campaign.py --start-engine
python demo_templates.py --start-engine
```

Engine env vars:

- `AXIOM_ASSETS_DIR` — custom asset directory for Bevy's asset loader (default: `assets`).
  Games that live outside the engine repo use this to point at their own assets folder.
  ```powershell
  $env:AXIOM_ASSETS_DIR="C:\Users\you\my-game\assets"; cargo run
  ```
  On Linux/macOS:
  ```bash
  AXIOM_ASSETS_DIR=/path/to/my-game/assets cargo run
  ```

Optional engine env vars for `--start-engine`:

- `AXIOM_ENGINE_CMD` (default: `cargo run -- --headless`)
- `AXIOM_ENGINE_CWD` (default: repo root)
- `AXIOM_ENGINE_LOGS=1` to keep engine stdout/stderr visible
- `AXIOM_ENGINE_REQUIRE_FRESH=1` to fail if an engine is already running at `AXIOM_API_URL`

## Scripts

- `demo.py`
  - Full autonomous loop (feel check/tune, generate, validate, solve, report).
  - Supports `--start-engine` and `--startup-timeout`.
- `demo_examples.py`
  - Loads every built-in example and runs a smoke check per example.
  - Supports `--start-engine` and `--startup-timeout`.
- `demo_replay.py`
  - Replay determinism smoke (`/replay/record -> /simulate -> /replay/stop -> /replay/play`).
  - Supports `--start-engine` and `--startup-timeout`.
- `demo_perf.py`
  - Spawns a large scripted crowd and prints `/perf`, `/perf/history`, and `/scripts/stats`.
  - Supports `--start-engine` and `--startup-timeout`.
- `demo_export.py`
  - Web/desktop export smoke: builds bundles and verifies generated artifacts.
  - Prints export compatibility details (`stripped_scripts`, `transpiled_scripts`, warnings).
  - Note: may take a few minutes on first run; `wasm-bindgen` is optional for web `ready=true`.
  - Supports `--start-engine` and `--startup-timeout`.
  - Use `python demo_export.py --require-ready` to fail when wasm-bindgen JS glue is missing.
- `demo_campaign.py`
  - Level-pack campaign smoke (`/levels/pack -> /start -> /solve -> /next -> /progress`).
  - Supports `--start-engine` and `--startup-timeout`.
- `demo_templates.py`
  - Generator template matrix smoke (`/game/load_level` + `/solve` across all templates).
  - Supports `--start-engine` and `--startup-timeout`.
  - Use `--min-solved N` to fail the script if fewer than `N` templates solve.

- `tools/web_export_runtime_smoke.py`
  - Browser runtime smoke for `export/web` (requires Playwright + Chromium).
  - Example: `python tools/web_export_runtime_smoke.py export/web --timeout 90`
- `tools/perf_long_horizon.py`
  - Sustained profiling across built-in examples; writes a JSON report.
  - Uses `/perf/history` by default (falls back to `/perf` polling). Pass `--no-perf-history` to force polling.
  - Supports `--thresholds-file <json>` for default + per-example budgets during long-horizon tuning.
  - Supports `--write-thresholds <json>` to emit auto-calibrated threshold suggestions from the current run.
  - Supports `--apply-thresholds <json>` to emit a bounded-blend calibrated thresholds file for direct reuse.
  - Example: `python tools/perf_long_horizon.py --start-engine --duration-per-example 8`
- `tools/template_seed_scout.py`
  - Finds first solver-success seed per template in a target seed range.
  - Example: `python tools/template_seed_scout.py --start-engine --seed-start 1 --seed-end 200 --write-json tools/template_seeds.json`
- `demo_topdown.py`
  - Top-down dungeon flow.
  - Supports `--start-engine` and `--startup-timeout`.
- `demo_rts.py`
  - RTS arena flow.
  - Supports `--start-engine` and `--startup-timeout`.
- `demo_fighting.py`
  - Fighting arena flow.
  - Supports `--start-engine` and `--startup-timeout`.
- `load_topdown.py`
  - Utility loader for a top-down dungeon map.
  - Supports `--start-engine` and `--startup-timeout`.

## Notes

- Demos assume API endpoints are enabled (desktop/headless runtime).
- For remote hosts, set `AXIOM_API_URL` before running a script.
