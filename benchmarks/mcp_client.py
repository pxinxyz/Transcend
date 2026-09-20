"""Minimal MCP stdio client, so the efficiency harness measures the real server.

Talks the same JSON-RPC handshake a host does: `initialize`, then
`notifications/initialized`, then `tools/call`. Returns the tool's text payload, which is
what a host hands to the model -- protocol framing is excluded from every measurement.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import threading
from typing import Any

PROTOCOL_VERSION = "2025-06-18"


class McpSession:
    """One long-lived server process, so session state (workspace, LSP warm-up) persists."""

    def __init__(self, binary: str, cwd: str, workspace: str | None = None) -> None:
        # Set TRANSCEND_MCP_STDERR to a path to capture the server's stderr (it writes logs
        # and debug traces there). Default is to discard, since stdio servers are chatty.
        stderr_target = os.environ.get("TRANSCEND_MCP_STDERR")
        self._stderr = open(stderr_target, "w", encoding="utf-8") if stderr_target else subprocess.DEVNULL
        self.proc = subprocess.Popen(
            [binary],
            cwd=cwd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self._stderr,
            text=True,
            encoding="utf-8",
            bufsize=1,
        )
        self._id = 0
        self._lock = threading.Lock()
        self._handshake(workspace)

    def _send(self, payload: dict[str, Any]) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(json.dumps(payload) + "\n")
        self.proc.stdin.flush()

    def _read_until(self, want_id: int) -> dict[str, Any]:
        assert self.proc.stdout is not None
        while True:
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError("server closed stdout before replying")
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                # Servers may emit non-JSON noise on stdout; skip rather than fail.
                continue
            if msg.get("id") == want_id:
                return msg

    def _call(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        with self._lock:
            self._id += 1
            want = self._id
            self._send(
                {"jsonrpc": "2.0", "id": want, "method": method, "params": params}
            )
            return self._read_until(want)

    def _notify(self, method: str, params: dict[str, Any]) -> None:
        self._send({"jsonrpc": "2.0", "method": method, "params": params})

    def _handshake(self, workspace: str | None) -> None:
        res = self._call(
            "initialize",
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "transcend-bench", "version": "1.0"},
            },
        )
        if "error" in res:
            raise RuntimeError(f"initialize failed: {res['error']}")
        self._notify("notifications/initialized", {})
        if workspace:
            self.call("set_workspace", {"path": workspace})

    def call(self, tool: str, arguments: dict[str, Any]) -> str:
        """Invoke a tool and return its text payload exactly as a host would see it."""
        res = self._call(
            "tools/call", {"name": tool, "arguments": arguments}
        )
        if "error" in res:
            return json.dumps(res["error"])
        result = res.get("result", {})
        content = result.get("content") or []
        parts = [c.get("text", "") for c in content if c.get("type") == "text"]
        payload = "\n".join(parts)
        if result.get("isError"):
            payload = f"[isError] {payload}"
        return payload

    def close(self) -> None:
        try:
            if self.proc.stdin:
                self.proc.stdin.close()
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()


def main() -> None:
    """Smoke test: point at a server and dump one tool call."""
    binary = sys.argv[1] if len(sys.argv) > 1 else "target/debug/transcend.exe"
    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    s = McpSession(binary, repo, workspace=repo)
    try:
        print(s.call("find", {"pattern": "*.rs", "options": {"max_results": 2}}))
    finally:
        s.close()


if __name__ == "__main__":
    main()
