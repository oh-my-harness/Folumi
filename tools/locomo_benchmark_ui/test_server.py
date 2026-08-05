from __future__ import annotations

import json
import os
import tempfile
import threading
import unittest
from http.client import HTTPConnection
from pathlib import Path
from unittest.mock import patch

from tools.locomo_benchmark_ui.server import (
    BenchmarkApplication,
    ConfigurationError,
    ProductAssistantProfile,
    build_command,
    build_environment,
    load_product_assistant_profile,
    make_handler,
    validate_run_config,
)
from http.server import ThreadingHTTPServer


class BenchmarkServerTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.repo_root = Path(self.temporary_directory.name)
        (self.repo_root / "Cargo.toml").write_text(
            'llm-harness-agent = { git = "https://example.invalid/runtime", rev = "abc123" }\n',
            encoding="utf-8",
        )
        self.dataset = self.repo_root / "locomo10.json"
        self.dataset.write_text("[]", encoding="utf-8")

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def test_retrieval_configuration_builds_targeted_serial_command(self) -> None:
        run = validate_run_config(
            self.repo_root,
            {
                "kind": "retrieval",
                "dataset": str(self.dataset),
                "profile": "release",
                "max_samples": 2,
                "max_questions": 7,
                "run_id": "retrieval-smoke",
            },
        )

        self.assertEqual(
            build_command(run),
            [
                "cargo",
                "test",
                "-p",
                "tutor-web",
                "--lib",
                "--release",
                "locomo_history_recall_retrieval_benchmark",
                "--",
                "--ignored",
                "--nocapture",
            ],
        )
        environment = build_environment(self.repo_root, run)
        self.assertEqual(environment["CARGO_BUILD_JOBS"], "1")
        self.assertEqual(environment["FOLUMI_LOCOMO_MAX_SAMPLES"], "2")
        self.assertEqual(environment["FOLUMI_LOCOMO_MAX_QUESTIONS"], "7")
        self.assertEqual(environment["FOLUMI_BENCHMARK_RUNTIME_REVISION"], "abc123")

    def test_answer_configuration_requires_key_and_passes_it_only_to_environment(self) -> None:
        payload = {
            "kind": "answer",
            "dataset": str(self.dataset),
            "profile": "debug",
            "run_id": "answer-smoke",
            "provider": "openai",
            "model": "test-model",
            "assistant_name": "Mori",
            "assistant_instructions": "Return only the requested answer.",
        }
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(ConfigurationError, "OPENAI_API_KEY"):
                validate_run_config(self.repo_root, payload)

            run = validate_run_config(self.repo_root, {**payload, "api_key": "temporary-secret"})
            environment = build_environment(self.repo_root, run)

        self.assertEqual(environment["OPENAI_API_KEY"], "temporary-secret")
        self.assertEqual(environment["FOLUMI_LOCOMO_ASSISTANT_NAME"], "Mori")
        self.assertEqual(
            environment["FOLUMI_LOCOMO_ASSISTANT_INSTRUCTIONS"],
            "Return only the requested answer.",
        )
        self.assertEqual(
            environment["FOLUMI_LOCOMO_ASSISTANT_PROFILE_SOURCE"],
            "benchmark_override",
        )
        self.assertNotIn("temporary-secret", " ".join(build_command(run)))

    def test_product_profile_is_loaded_without_exposing_other_settings(self) -> None:
        settings_path = self.repo_root / "desktop-settings.json"
        settings_path.write_text(
            json.dumps(
                {
                    "assistantName": "峰哥",
                    "assistantInstructions": "像一个可靠的长期搭档。",
                    "apiKey": "must-not-leak",
                },
                ensure_ascii=False,
            ),
            encoding="utf-8",
        )

        profile = load_product_assistant_profile(self.repo_root, settings_path)
        self.assertEqual(
            profile,
            ProductAssistantProfile("峰哥", "像一个可靠的长期搭档。"),
        )
        application = BenchmarkApplication(self.repo_root, 0, settings_path)
        serialized_state = json.dumps(application.state(), ensure_ascii=False)
        self.assertIn("峰哥", serialized_state)
        self.assertNotIn("像一个可靠的长期搭档。", serialized_state)
        self.assertNotIn("must-not-leak", serialized_state)

    def test_answer_can_use_current_product_profile(self) -> None:
        profile = ProductAssistantProfile("峰哥", "像一个可靠的长期搭档。")
        payload = {
            "kind": "answer",
            "dataset": str(self.dataset),
            "run_id": "product-profile",
            "provider": "openai",
            "model": "test-model",
            "api_key": "temporary-secret",
            "assistant_profile_mode": "product",
        }

        run = validate_run_config(self.repo_root, payload, profile)
        environment = build_environment(self.repo_root, run)

        self.assertEqual(environment["FOLUMI_LOCOMO_ASSISTANT_NAME"], "峰哥")
        self.assertEqual(
            environment["FOLUMI_LOCOMO_ASSISTANT_INSTRUCTIONS"],
            "像一个可靠的长期搭档。",
        )
        self.assertEqual(
            environment["FOLUMI_LOCOMO_ASSISTANT_PROFILE_SOURCE"],
            "product_settings",
        )

    def test_existing_output_is_never_overwritten(self) -> None:
        output = self.repo_root / "benchmarks/locomo/results/repeated.json"
        output.parent.mkdir(parents=True)
        output.write_text("{}", encoding="utf-8")

        with self.assertRaisesRegex(ConfigurationError, "已存在"):
            validate_run_config(
                self.repo_root,
                {
                    "kind": "retrieval",
                    "dataset": str(self.dataset),
                    "run_id": "repeated",
                },
            )

    def test_http_api_serves_ui_and_rejects_invalid_token(self) -> None:
        chart = self.repo_root / "benchmarks/locomo/charts/retrieval-comparison.svg"
        chart.parent.mkdir(parents=True)
        chart.write_text("<svg xmlns='http://www.w3.org/2000/svg'></svg>", encoding="utf-8")
        application = BenchmarkApplication(self.repo_root, 0)
        server = ThreadingHTTPServer(("127.0.0.1", 0), make_handler(application))
        application.port = server.server_port
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()

        def request(method: str, path: str, body: bytes | None = None, headers=None):
            connection = HTTPConnection("127.0.0.1", server.server_port, timeout=3)
            try:
                connection.request(method, path, body=body, headers=headers or {})
                response = connection.getresponse()
                return response.status, dict(response.getheaders()), response.read()
            finally:
                connection.close()

        try:
            status, _, body = request("GET", "/api/state")
            state = json.loads(body.decode("utf-8"))
            self.assertEqual(status, 200)
            self.assertEqual(state["repo_root"], str(self.repo_root.resolve()))

            status, _, _ = request(
                "POST",
                "/api/run",
                body=b"{}",
                headers={
                    "Content-Type": "application/json",
                    "Content-Length": "2",
                    "X-Benchmark-Token": "wrong-token",
                },
            )
            self.assertEqual(status, 403)

            status, headers, body = request("GET", "/")
            page = body.decode("utf-8")
            self.assertEqual(status, 200)
            self.assertIn("LoCoMo Benchmark", page)
            self.assertIn("default-src 'self'", headers["Content-Security-Policy"])

            status, headers, _ = request("GET", "/api/chart/retrieval")
            self.assertEqual(status, 200)
            self.assertEqual(headers["Content-Type"], "image/svg+xml")
        finally:
            server.shutdown()
            server.server_close()
            worker.join(timeout=3)


if __name__ == "__main__":
    unittest.main()
