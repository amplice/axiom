import json
import http.client
import os
import pathlib
import shlex
import subprocess
import time
import urllib.error
import urllib.request
import math
from contextlib import contextmanager

DEFAULT_API_URL = "http://127.0.0.1:3000"


class DemoClient:
    def __init__(self, base_url: str = DEFAULT_API_URL, timeout: float = 30.0):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self.api_token = os.environ.get("AXIOM_API_TOKEN", "").strip()
        self.max_retries = max(0, int(os.environ.get("AXIOM_API_RETRIES", "2")))
        self.retry_backoff_ms = max(10, int(os.environ.get("AXIOM_API_RETRY_BACKOFF_MS", "150")))

    def _request_once(self, method: str, path: str, data=None):
        url = f"{self.base_url}{path}"
        headers = {"Connection": "close"}
        if self.api_token:
            headers["Authorization"] = f"Bearer {self.api_token}"
        body = None
        if data is not None:
            body = json.dumps(data).encode()
            headers["Content-Type"] = "application/json"
        req = urllib.request.Request(url, data=body, headers=headers, method=method)
        with urllib.request.urlopen(req, timeout=self.timeout) as response:
            return json.loads(response.read())

    def _request(self, method: str, path: str, data=None):
        is_write = method in {"POST", "PUT", "PATCH", "DELETE"}
        retries = 0 if is_write else self.max_retries

        attempt = 0
        while True:
            attempt += 1
            try:
                return self._request_once(method, path, data)
            except urllib.error.HTTPError as exc:
                # Retry only on transient server failures.
                if exc.code < 500 or attempt > retries + 1:
                    raise
            except (
                urllib.error.URLError,
                TimeoutError,
                ConnectionResetError,
                ConnectionAbortedError,
                http.client.RemoteDisconnected,
            ):
                if attempt > retries + 1:
                    raise

            delay = (self.retry_backoff_ms / 1000.0) * (2 ** (attempt - 1))
            time.sleep(min(delay, 2.0))

    def get(self, path: str):
        return self._request("GET", path)

    def post(self, path: str, data=None):
        payload = {} if data is None else data
        return self._request("POST", path, payload)

    def delete(self, path: str):
        return self._request("DELETE", path)

    def wait_for_server(self, timeout: float = 10.0, probe_path: str = "/state") -> bool:
        start = time.time()
        while time.time() - start < timeout:
            try:
                self.get(probe_path)
                return True
            except Exception:
                time.sleep(0.5)
        return False


def create_client(timeout: float = 30.0) -> DemoClient:
    return DemoClient(os.environ.get("AXIOM_API_URL", DEFAULT_API_URL), timeout=timeout)


def build_top_down_inputs(path_points, move_speed: float, start_point=None, axis_order: str = "yx"):
    units_per_frame = max(move_speed / 60.0, 1.0)
    frame = 0
    inputs = []
    residual_x = 0.0
    residual_y = 0.0
    if not path_points:
        return inputs

    x0, y0 = start_point if start_point is not None else path_points[0]
    for x1, y1 in path_points:
        dx = x1 - x0
        dy = y1 - y0
        axis_steps = []
        if axis_order == "xy":
            axis_steps = [("x", dx), ("y", dy)]
        else:
            axis_steps = [("y", dy), ("x", dx)]

        for axis, delta in axis_steps:
            if abs(delta) <= 1.0:
                continue
            action = ("right" if delta > 0 else "left") if axis == "x" else ("up" if delta > 0 else "down")
            exact = abs(delta) / units_per_frame
            if axis == "x":
                exact += residual_x
            else:
                exact += residual_y
            duration = max(1, int(round(exact)))
            if axis == "x":
                residual_x = exact - duration
            else:
                residual_y = exact - duration
            inputs.append({"frame": frame, "action": action, "duration": duration})
            frame += duration

        x0, y0 = x1, y1

    return inputs


def reset_entities(client: DemoClient, delete_players: bool = False) -> dict:
    reset = client.post("/entities/reset_non_player", {})
    if not reset.get("ok"):
        raise RuntimeError(reset.get("error", "failed to reset non-player entities"))

    players_removed = 0
    if delete_players:
        entities = client.get("/entities").get("data") or []
        for entity in entities:
            if "Player" not in entity.get("components", []):
                continue
            retries = 0
            while True:
                retries += 1
                try:
                    client.delete(f"/entities/{entity['id']}")
                    players_removed += 1
                    break
                except urllib.error.HTTPError as exc:
                    if exc.code == 429 and retries < 8:
                        time.sleep(0.02 * retries)
                        continue
                    raise

    return {"players_removed": players_removed}


@contextmanager
def managed_engine(
    client: DemoClient,
    start_engine: bool = False,
    startup_timeout: float = 30.0,
    probe_path: str = "/state",
):
    proc = None
    try:
        if start_engine:
            existing_ready = client.wait_for_server(timeout=1.0, probe_path=probe_path)
            require_fresh = os.environ.get("AXIOM_ENGINE_REQUIRE_FRESH", "").strip().lower() in {
                "1",
                "true",
                "yes",
                "on",
            }
            if existing_ready:
                if require_fresh:
                    raise RuntimeError(
                        "AXIOM_API_URL is already serving. Stop the existing engine or unset AXIOM_ENGINE_REQUIRE_FRESH."
                    )
                # Reuse existing server when available instead of spawning a conflicting process.
                start_engine = False

        if start_engine:
            cmd = os.environ.get("AXIOM_ENGINE_CMD", "cargo run -- --headless").strip()
            cwd = os.environ.get("AXIOM_ENGINE_CWD", str(pathlib.Path(__file__).resolve().parent))
            show_logs = os.environ.get("AXIOM_ENGINE_LOGS", "").strip().lower() in {
                "1",
                "true",
                "yes",
                "on",
            }
            stdout = None if show_logs else subprocess.DEVNULL
            stderr = None if show_logs else subprocess.DEVNULL
            cmd_parts = shlex.split(cmd, posix=(os.name != "nt"))
            if not cmd_parts:
                raise RuntimeError("AXIOM_ENGINE_CMD cannot be empty.")
            proc = subprocess.Popen(cmd_parts, cwd=cwd, stdout=stdout, stderr=stderr)
            time.sleep(0.2)
            if proc.poll() is not None:
                raise RuntimeError(
                    f"Failed to start engine command '{cmd}' (exit code {proc.returncode})."
                )

        if not client.wait_for_server(timeout=startup_timeout, probe_path=probe_path):
            if proc is not None and proc.poll() is not None:
                raise RuntimeError(
                    f"Engine process exited before API became ready (exit code {proc.returncode})."
                )
            raise RuntimeError(
                "Engine is not reachable at AXIOM_API_URL (or default http://127.0.0.1:3000)."
            )
        yield
    finally:
        if proc is not None and proc.poll() is None:
            if os.name == "nt":
                subprocess.run(
                    ["taskkill", "/PID", str(proc.pid), "/T", "/F"],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False,
                )
            else:
                proc.terminate()
                try:
                    proc.wait(timeout=8.0)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=4.0)
