#!/usr/bin/env python3
"""Local-only LoCoMo benchmark dashboard using only the Python standard library."""

from __future__ import annotations

import argparse
import json
import os
import re
import secrets
import signal
import shutil
import subprocess
import threading
import webbrowser
from dataclasses import dataclass
from datetime import datetime, timezone
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlparse


MAX_BODY_BYTES = 64 * 1024
MAX_LOG_LINES = 2_000
RUN_ID_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$")
BENCHMARKS = {
    "retrieval": {
        "test": "locomo_history_recall_retrieval_benchmark",
        "output_env": "FOLUMI_LOCOMO_OUTPUT",
        "results_dir": Path("benchmarks/locomo/results"),
        "benchmark": "locomo_history_recall_retrieval",
        "renderer": Path("scripts/render-locomo-benchmarks.ps1"),
        "chart": Path("benchmarks/locomo/charts/retrieval-comparison.svg"),
    },
    "answer": {
        "test": "locomo_agent_answer_accuracy_benchmark",
        "output_env": "FOLUMI_LOCOMO_ANSWER_OUTPUT",
        "results_dir": Path("benchmarks/locomo/answer-results"),
        "benchmark": "locomo_agent_answer_accuracy",
        "renderer": Path("scripts/render-locomo-answer-benchmarks.ps1"),
        "chart": Path("benchmarks/locomo/charts/answer-comparison.svg"),
    },
}


class ConfigurationError(ValueError):
    pass


@dataclass(frozen=True)
class ValidatedRun:
    kind: str
    dataset: Path
    profile: str
    max_samples: int | None
    max_questions: int | None
    run_id: str
    output_path: Path
    provider: str | None
    model: str | None
    api_key: str | None
    base_url: str | None
    chat_path: str | None
    include_text: bool


def utc_now() -> datetime:
    return datetime.now(timezone.utc)


def positive_optional_int(value: Any, name: str) -> int | None:
    if value in (None, ""):
        return None
    try:
        parsed = int(value)
    except (TypeError, ValueError) as error:
        raise ConfigurationError(f"{name} 必须是正整数") from error
    if parsed <= 0 or parsed > 100_000:
        raise ConfigurationError(f"{name} 必须在 1 到 100000 之间")
    return parsed


def validate_run_config(repo_root: Path, payload: dict[str, Any]) -> ValidatedRun:
    kind = str(payload.get("kind", "")).strip().lower()
    if kind not in BENCHMARKS:
        raise ConfigurationError("评测类型必须是 retrieval 或 answer")

    dataset_raw = str(payload.get("dataset", "")).strip().strip('"')
    if not dataset_raw:
        raise ConfigurationError("请选择 LoCoMo locomo10.json")
    dataset = Path(dataset_raw).expanduser().resolve()
    if not dataset.is_file():
        raise ConfigurationError(f"数据集文件不存在：{dataset}")

    profile = str(payload.get("profile", "debug")).strip().lower()
    if profile not in {"debug", "release"}:
        raise ConfigurationError("构建模式必须是 debug 或 release")

    max_samples = positive_optional_int(payload.get("max_samples"), "Sample 数")
    max_questions = positive_optional_int(payload.get("max_questions"), "每组题目数")
    requested_run_id = str(payload.get("run_id", "")).strip()
    default_run_id = f"{kind}-{utc_now().strftime('%Y%m%dT%H%M%SZ')}"
    run_id = requested_run_id or default_run_id
    if not RUN_ID_PATTERN.fullmatch(run_id):
        raise ConfigurationError("Run ID 只能包含字母、数字、点、下划线和短横线，且不超过 80 个字符")

    config = BENCHMARKS[kind]
    output_dir = (repo_root / config["results_dir"]).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / f"{run_id}.json"
    if output_path.exists():
        raise ConfigurationError(f"结果文件已存在，请更换 Run ID：{output_path.name}")

    provider = model = api_key = base_url = chat_path = None
    include_text = bool(payload.get("include_text", False))
    if kind == "answer":
        provider = str(payload.get("provider", "")).strip().lower()
        if provider not in {"anthropic", "openai", "deepseek"}:
            raise ConfigurationError("回答评测必须选择 anthropic、openai 或 deepseek")
        model = str(payload.get("model", "")).strip()
        if not model:
            raise ConfigurationError("回答评测必须填写固定的模型 ID")
        api_key = str(payload.get("api_key", "")).strip() or None
        base_url = str(payload.get("base_url", "")).strip() or None
        chat_path = str(payload.get("chat_path", "")).strip() or None
        key_env = provider_key_env(provider)
        if api_key is None and not os.environ.get(key_env):
            raise ConfigurationError(f"请填写 API Key，或在启动控制台前设置 {key_env}")

    return ValidatedRun(
        kind=kind,
        dataset=dataset,
        profile=profile,
        max_samples=max_samples,
        max_questions=max_questions,
        run_id=run_id,
        output_path=output_path,
        provider=provider,
        model=model,
        api_key=api_key,
        base_url=base_url,
        chat_path=chat_path,
        include_text=include_text,
    )


def provider_key_env(provider: str) -> str:
    return {
        "anthropic": "ANTHROPIC_API_KEY",
        "openai": "OPENAI_API_KEY",
        "deepseek": "DEEPSEEK_API_KEY",
    }[provider]


def git_revision(path: Path) -> str:
    try:
        return subprocess.run(
            ["git", "-C", str(path), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return "unknown"


def dataset_revision(dataset: Path) -> str:
    try:
        root = subprocess.run(
            ["git", "-C", str(dataset.parent), "rev-parse", "--show-toplevel"],
            check=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return "unknown"
    return git_revision(Path(root))


def runtime_revision(repo_root: Path) -> str:
    manifest = (repo_root / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'llm-harness-agent\s*=\s*\{[^\n]*rev\s*=\s*"([0-9a-f]+)"', manifest)
    return match.group(1) if match else "unknown"


def build_environment(repo_root: Path, run: ValidatedRun) -> dict[str, str]:
    env = os.environ.copy()
    env["CARGO_BUILD_JOBS"] = "1"
    env["FOLUMI_LOCOMO_DATASET"] = str(run.dataset)
    env["FOLUMI_BENCHMARK_RUN_ID"] = run.run_id
    env["FOLUMI_BENCHMARK_FOLUMI_REVISION"] = git_revision(repo_root)
    env["FOLUMI_BENCHMARK_RUNTIME_REVISION"] = runtime_revision(repo_root)
    env["FOLUMI_BENCHMARK_LOCOMO_REVISION"] = dataset_revision(run.dataset)
    env[BENCHMARKS[run.kind]["output_env"]] = str(run.output_path)
    for name, value in (
        ("FOLUMI_LOCOMO_MAX_SAMPLES", run.max_samples),
        ("FOLUMI_LOCOMO_MAX_QUESTIONS", run.max_questions),
    ):
        if value is None:
            env.pop(name, None)
        else:
            env[name] = str(value)

    if run.kind == "answer":
        assert run.provider and run.model
        env["LLM_PROVIDER"] = run.provider
        env["LLM_MODEL"] = run.model
        if run.api_key:
            env[provider_key_env(run.provider)] = run.api_key
        base_env, chat_env = {
            "anthropic": ("ANTHROPIC_BASE_URL", None),
            "openai": ("OPENAI_BASE_URL", "OPENAI_CHAT_PATH"),
            "deepseek": ("DEEPSEEK_API_BASE", "DEEPSEEK_CHAT_PATH"),
        }[run.provider]
        if run.base_url:
            env[base_env] = run.base_url
        if chat_env and run.chat_path:
            env[chat_env] = run.chat_path
        if run.include_text:
            env["FOLUMI_LOCOMO_INCLUDE_TEXT"] = "true"
        else:
            env.pop("FOLUMI_LOCOMO_INCLUDE_TEXT", None)
    return env


def build_command(run: ValidatedRun) -> list[str]:
    command = ["cargo", "test", "-p", "tutor-web", "--lib"]
    if run.profile == "release":
        command.append("--release")
    command.extend([BENCHMARKS[run.kind]["test"], "--", "--ignored", "--nocapture"])
    return command


class RunManager:
    def __init__(self, repo_root: Path):
        self.repo_root = repo_root
        self.lock = threading.Lock()
        self.process: subprocess.Popen[str] | None = None
        self.worker: threading.Thread | None = None
        self.phase = "idle"
        self.kind: str | None = None
        self.run_id: str | None = None
        self.output_path: str | None = None
        self.started_at: str | None = None
        self.ended_at: str | None = None
        self.exit_code: int | None = None
        self.error: str | None = None
        self.logs: list[str] = []
        self.stop_requested = False

    def start(self, run: ValidatedRun) -> None:
        with self.lock:
            if self.phase in {"starting", "running", "stopping"}:
                raise ConfigurationError("已有评测正在运行，请等待完成或先停止")
            self.phase = "starting"
            self.kind = run.kind
            self.run_id = run.run_id
            self.output_path = str(run.output_path)
            self.started_at = utc_now().isoformat()
            self.ended_at = None
            self.exit_code = None
            self.error = None
            self.logs = []
            self.stop_requested = False
            self.worker = threading.Thread(target=self._run, args=(run,), daemon=True)
            self.worker.start()

    def _append_log(self, line: str) -> None:
        with self.lock:
            self.logs.append(line.rstrip("\r\n"))
            if len(self.logs) > MAX_LOG_LINES:
                del self.logs[: len(self.logs) - MAX_LOG_LINES]

    def _run(self, run: ValidatedRun) -> None:
        command = build_command(run)
        self._append_log(f"$ {' '.join(command)}")
        self._append_log(f"结果文件：{run.output_path}")
        try:
            creation_flags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
            process = subprocess.Popen(
                command,
                cwd=self.repo_root,
                env=build_environment(self.repo_root, run),
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
                creationflags=creation_flags,
                start_new_session=os.name != "nt",
            )
            with self.lock:
                self.process = process
                should_stop = self.stop_requested
                self.phase = "stopping" if should_stop else "running"
            if should_stop:
                self._terminate_process(process)
            assert process.stdout is not None
            for line in process.stdout:
                self._append_log(line)
            exit_code = process.wait()
            with self.lock:
                stopped = self.stop_requested
                self.exit_code = exit_code
                self.phase = "stopped" if stopped else ("running" if exit_code == 0 else "failed")
            if exit_code == 0 and not stopped:
                self._render_chart(run.kind)
                with self.lock:
                    self.phase = "succeeded"
        except Exception as error:  # Keep the local controller alive and report the exact failure.
            self._append_log(f"控制台错误：{error}")
            with self.lock:
                self.error = str(error)
                self.phase = "failed"
        finally:
            with self.lock:
                self.process = None
                self.ended_at = utc_now().isoformat()

    def _render_chart(self, kind: str) -> None:
        renderer = self.repo_root / BENCHMARKS[kind]["renderer"]
        shell = shutil.which("pwsh") or shutil.which("powershell")
        if not shell:
            self._append_log("未找到 PowerShell，已跳过图表生成。")
            return
        self._append_log("正在更新对比图……")
        completed = subprocess.run(
            [shell, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(renderer)],
            cwd=self.repo_root,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        if completed.stdout.strip():
            self._append_log(completed.stdout.strip())
        if completed.returncode != 0:
            self._append_log(f"图表生成失败：{completed.stderr.strip()}")

    @staticmethod
    def _terminate_process(process: subprocess.Popen[str]) -> None:
        if os.name == "nt":
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                capture_output=True,
                check=False,
            )
        else:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass

    def stop(self) -> bool:
        with self.lock:
            process = self.process
            if self.phase not in {"starting", "running"}:
                return False
            self.stop_requested = True
            self.phase = "stopping"
        if process:
            self._terminate_process(process)
        return True

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {
                "phase": self.phase,
                "kind": self.kind,
                "run_id": self.run_id,
                "output_path": self.output_path,
                "started_at": self.started_at,
                "ended_at": self.ended_at,
                "exit_code": self.exit_code,
                "error": self.error,
                "logs": list(self.logs),
            }


class BenchmarkApplication:
    def __init__(self, repo_root: Path, port: int):
        self.repo_root = repo_root.resolve()
        self.static_root = Path(__file__).resolve().parent / "static"
        self.port = port
        self.token = secrets.token_urlsafe(32)
        self.manager = RunManager(self.repo_root)

    def state(self) -> dict[str, Any]:
        return {
            "token": self.token,
            "repo_root": str(self.repo_root),
            "default_dataset": os.environ.get("FOLUMI_LOCOMO_DATASET", ""),
            "provider_keys": {
                provider: bool(os.environ.get(provider_key_env(provider)))
                for provider in ("anthropic", "openai", "deepseek")
            },
        }

    def results(self) -> dict[str, Any]:
        runs: list[dict[str, Any]] = []
        warnings: list[str] = []
        charts: dict[str, str | None] = {}
        for kind, config in BENCHMARKS.items():
            directory = self.repo_root / config["results_dir"]
            directory.mkdir(parents=True, exist_ok=True)
            for path in directory.glob("*.json"):
                try:
                    report = json.loads(path.read_text(encoding="utf-8"))
                    if report.get("schema_version") != 1 or report.get("benchmark") != config["benchmark"]:
                        warnings.append(f"忽略不支持的结果：{path.name}")
                        continue
                    runs.append(summarize_result(kind, path.name, report))
                except (OSError, json.JSONDecodeError) as error:
                    warnings.append(f"读取 {path.name} 失败：{error}")
            chart = self.repo_root / config["chart"]
            charts[kind] = f"/api/chart/{kind}?v={chart.stat().st_mtime_ns}" if chart.is_file() else None
        runs.sort(key=lambda run: (run.get("generated_at") or "", run.get("run_id") or ""), reverse=True)
        return {"runs": runs, "charts": charts, "warnings": warnings}


def summarize_result(kind: str, filename: str, report: dict[str, Any]) -> dict[str, Any]:
    return {
        "kind": kind,
        "filename": filename,
        "run_id": report.get("run_id"),
        "generated_at": report.get("generated_at"),
        "profile": report.get("profile"),
        "provenance": report.get("provenance", {}),
        "configuration": report.get("configuration", {}),
        "dataset_counts": report.get("dataset_counts", {}),
        "overall": report.get("overall", {}),
        "categories": report.get("categories", {}),
        "diagnostics": report.get("diagnostics", {}),
        "download_url": f"/api/result/{kind}/{filename}",
    }


def make_handler(application: BenchmarkApplication):
    class Handler(BaseHTTPRequestHandler):
        server_version = "FolumiBenchmark/1"

        def log_message(self, format: str, *args: Any) -> None:
            return

        def end_headers(self) -> None:
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.send_header("Referrer-Policy", "no-referrer")
            self.send_header(
                "Content-Security-Policy",
                "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'self'",
            )
            super().end_headers()

        def do_GET(self) -> None:
            path = urlparse(self.path).path
            if path == "/api/state":
                self._json(application.state())
            elif path == "/api/status":
                self._json(application.manager.snapshot())
            elif path == "/api/results":
                self._json(application.results())
            elif path.startswith("/api/chart/"):
                self._serve_chart(path[len("/api/chart/") :])
            elif path.startswith("/api/result/"):
                self._serve_result(path[len("/api/result/") :])
            else:
                self._serve_static(path)

        def do_POST(self) -> None:
            if self.headers.get("X-Benchmark-Token") != application.token:
                self._json({"error": "请求令牌无效，请刷新页面"}, HTTPStatus.FORBIDDEN)
                return
            origin = self.headers.get("Origin")
            allowed_origins = {
                f"http://127.0.0.1:{application.port}",
                f"http://localhost:{application.port}",
            }
            if origin and origin not in allowed_origins:
                self._json({"error": "拒绝来自其他站点的请求"}, HTTPStatus.FORBIDDEN)
                return
            try:
                payload = self._read_json()
                if self.path == "/api/run":
                    run = validate_run_config(application.repo_root, payload)
                    application.manager.start(run)
                    self._json({"ok": True, "run_id": run.run_id}, HTTPStatus.ACCEPTED)
                elif self.path == "/api/stop":
                    stopped = application.manager.stop()
                    self._json({"ok": stopped})
                else:
                    self._json({"error": "接口不存在"}, HTTPStatus.NOT_FOUND)
            except ConfigurationError as error:
                self._json({"error": str(error)}, HTTPStatus.BAD_REQUEST)
            except (json.JSONDecodeError, UnicodeDecodeError):
                self._json({"error": "请求 JSON 无效"}, HTTPStatus.BAD_REQUEST)

        def _read_json(self) -> dict[str, Any]:
            length = int(self.headers.get("Content-Length", "0"))
            if length <= 0 or length > MAX_BODY_BYTES:
                raise ConfigurationError("请求大小无效")
            value = json.loads(self.rfile.read(length).decode("utf-8"))
            if not isinstance(value, dict):
                raise ConfigurationError("请求必须是 JSON 对象")
            return value

        def _json(self, value: Any, status: HTTPStatus = HTTPStatus.OK) -> None:
            body = json.dumps(value, ensure_ascii=False).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def _serve_static(self, request_path: str) -> None:
            filename = {"/": "index.html", "/app.js": "app.js", "/styles.css": "styles.css"}.get(request_path)
            if not filename:
                self.send_error(HTTPStatus.NOT_FOUND)
                return
            self._send_file(application.static_root / filename)

        def _serve_chart(self, kind: str) -> None:
            if kind not in BENCHMARKS:
                self.send_error(HTTPStatus.NOT_FOUND)
                return
            self._send_file(application.repo_root / BENCHMARKS[kind]["chart"])

        def _serve_result(self, suffix: str) -> None:
            try:
                kind, encoded_name = suffix.split("/", 1)
            except ValueError:
                self.send_error(HTTPStatus.NOT_FOUND)
                return
            filename = unquote(encoded_name)
            if kind not in BENCHMARKS or Path(filename).name != filename or not filename.endswith(".json"):
                self.send_error(HTTPStatus.NOT_FOUND)
                return
            self._send_file(application.repo_root / BENCHMARKS[kind]["results_dir"] / filename, download=True)

        def _send_file(self, path: Path, download: bool = False) -> None:
            if not path.is_file():
                self.send_error(HTTPStatus.NOT_FOUND)
                return
            body = path.read_bytes()
            content_type = {
                ".html": "text/html; charset=utf-8",
                ".js": "text/javascript; charset=utf-8",
                ".css": "text/css; charset=utf-8",
                ".svg": "image/svg+xml",
                ".json": "application/json; charset=utf-8",
            }.get(path.suffix.lower(), "application/octet-stream")
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            if download:
                self.send_header("Content-Disposition", f'attachment; filename="{path.name}"')
            self.end_headers()
            self.wfile.write(body)

    return Handler


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Folumi LoCoMo benchmark 本地控制台")
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--no-browser", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = args.repo_root.resolve()
    if not (repo_root / "Cargo.toml").is_file():
        raise SystemExit(f"不是 Folumi 仓库：{repo_root}")
    if args.port < 0 or args.port > 65535:
        raise SystemExit("端口必须在 0 到 65535 之间")
    application = BenchmarkApplication(repo_root, args.port)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), make_handler(application))
    application.port = server.server_port
    url = f"http://127.0.0.1:{server.server_port}/"
    print(f"LoCoMo Benchmark 控制台：{url}", flush=True)
    print("按 Ctrl+C 停止。API Key 只保存在当前进程内存和评测子进程环境中。", flush=True)
    if not args.no_browser:
        threading.Timer(0.4, lambda: webbrowser.open(url)).start()
    try:
        server.serve_forever(poll_interval=0.25)
    except KeyboardInterrupt:
        print("\n正在关闭……", flush=True)
    finally:
        application.manager.stop()
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
