#!/usr/bin/env python3
"""Call the built C ABI through ctypes. Python is only a test runner.

Run: python3 crates/seattrellis-native-bridge/tests/abi_smoke.py <library>
Accepts .so, .dylib and .dll produced by `cargo build -p
seattrellis-native-bridge`; no third-party Python dependencies are required.
"""

from __future__ import annotations

import base64
import ctypes
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


class Buffer(ctypes.Structure):
    _fields_ = [("data", ctypes.POINTER(ctypes.c_uint8)), ("len", ctypes.c_size_t)]


class Bridge:
    def __init__(self, library: str):
        self.lib = ctypes.CDLL(str(Path(library).resolve()))
        self.lib.seattrellis_abi_version.argtypes = []
        self.lib.seattrellis_abi_version.restype = ctypes.c_uint32
        self.lib.seattrellis_session_create.argtypes = []
        self.lib.seattrellis_session_create.restype = ctypes.c_uint64
        self.lib.seattrellis_session_destroy.argtypes = [ctypes.c_uint64]
        self.lib.seattrellis_session_destroy.restype = None
        self.lib.seattrellis_session_cancel.argtypes = [ctypes.c_uint64]
        self.lib.seattrellis_session_cancel.restype = ctypes.c_int32
        self.lib.seattrellis_session_dispatch.argtypes = [
            ctypes.c_uint64, ctypes.POINTER(ctypes.c_uint8), ctypes.c_size_t
        ]
        self.lib.seattrellis_session_dispatch.restype = Buffer
        self.lib.seattrellis_buffer_free.argtypes = [Buffer]
        self.lib.seattrellis_buffer_free.restype = None
        assert self.lib.seattrellis_abi_version() == 1

    def raw(self, session: int, data: bytes):
        memory = (ctypes.c_uint8 * len(data)).from_buffer_copy(data)
        return self.lib.seattrellis_session_dispatch(session, memory, len(data))

    def decode(self, buffer: Buffer):
        assert bool(buffer.data), "native response quota exhausted"
        try:
            assert 0 < buffer.len <= 32 * 1024 * 1024
            return json.loads(ctypes.string_at(buffer.data, buffer.len))
        finally:
            self.lib.seattrellis_buffer_free(buffer)

    def call(self, session: int, operation: str, payload):
        envelope = {"protocol_version": 1, "operation": operation, "payload": payload}
        return self.decode(self.raw(session, json.dumps(envelope, ensure_ascii=False).encode("utf-8")))


def source():
    return {
        "api_version": 2, "student_count": 2, "seed": 42,
        "seat_positions": [[0, 0], [1, 0], [2, 0]], "fixed_seats": [[0, 0]],
        "students": [{"key": "S1", "display_name": "张同学"}, {"key": "S2", "display_name": "李同学"}],
        "layout": {"name": "本地班级", "seats": [
            {"seat_id": f"A{col}", "row": 1, "col": col, "enabled": True} for col in range(1, 4)
        ]},
    }


def success(response):
    assert response.get("protocol_version") == 1
    assert response.get("ok") is True, response.get("error")
    return response["result"]


def command(draft: str, revision: int, command_id: str, operations):
    return {
        "kind": "seattrellis_editor_command", "protocol_version": "1.0",
        "draft_id": draft, "base_revision": revision, "command_id": command_id,
        "action": "apply", "operations": operations,
    }


def lifecycle(bridge: Bridge):
    sessions = []
    try:
        first = bridge.lib.seattrellis_session_create()
        second = bridge.lib.seattrellis_session_create()
        sessions.extend([first, second])
        assert first and second and first != second
        generated = success(bridge.call(first, "generate", source()))
        assert generated["status"] == "Solved" and generated["feasible"] is True
        draft = generated["editor"]["draft_id"]
        assert bridge.call(second, "state", {"draft_id": draft})["error"]["status"] == 404
        locked = success(bridge.call(first, "command", command(draft, 0, "ctypes-lock", [
            {"kind": "lock_student", "payload": {"student_key": "S2"}}
        ])))
        assert locked["revision"] == 1
        moved = success(bridge.call(first, "command", command(draft, 1, "ctypes-move", [
            {"kind": "move_student", "payload": {"student_key": "S1", "seat_id": "A3"}}
        ])))
        assert moved["revision"] == 2 and moved["validation"]["valid"] is False
        document = success(bridge.call(first, "serialize", {
            "class_source": {"solve": source(), "notes": "只保存在完整班级源中"},
            "draft_refs": [{"draft_id": draft, "revision": 2}],
        }))
        assert document["drafts"][0]["lock_state"]["locked_students"] == ["S2"]
        bridge.lib.seattrellis_session_destroy(first)
        assert bridge.call(first, "state", {"draft_id": draft})["error"]["code"] == "invalid_session"
        recreated = bridge.lib.seattrellis_session_create()
        sessions.append(recreated)
        assert recreated and recreated != first
        bridge.lib.seattrellis_session_destroy(first)
        assert bridge.lib.seattrellis_session_cancel(recreated) == 0
        opened = success(bridge.call(recreated, "open", document))
        assert opened["class_source"]["notes"] == "只保存在完整班级源中"
        reopened_id = opened["editor"]["draft_id"]
        assert reopened_id != draft
        locked_seat = next(s for s in opened["editor"]["students"] if s["student_key"] == "S2")["seat_id"]
        assert success(bridge.call(recreated, "audit", {"draft_id": reopened_id}))["feasible"] is False
        assert bridge.call(recreated, "export", {"draft_id": reopened_id, "format": "svg"})["error"]["status"] == 422
        repaired = success(bridge.call(recreated, "repair", {
            "draft_id": reopened_id, "base_revision": 0, "affected_students": ["S1"]
        }))
        assert repaired["revision"] == 1
        students = {s["student_key"]: s for s in repaired["students"]}
        assert students["S1"]["seat_id"] == "A1"
        assert students["S2"]["seat_id"] == locked_seat and students["S2"]["locked"]
        assert success(bridge.call(recreated, "audit", {"draft_id": reopened_id}))["feasible"] is True
        exported = success(bridge.call(recreated, "export", {
            "draft_id": reopened_id, "format": "svg", "options": {
                "expected_revision": 1, "template": "public", "privacy": {"anonymize": True}
            }
        }))
        svg = base64.b64decode(exported["base64"], validate=True).decode("utf-8")
        assert exported["filename"] == "seat-plan.svg" and exported["mime_type"] == "image/svg+xml"
        assert "<svg" in svg and "张同学" not in svg and "李同学" not in svg
        assert bridge.call(recreated, "export", {
            "draft_id": reopened_id, "format": "svg", "options": {"request": {}}
        })["error"]["status"] == 400
        assert success(bridge.call(recreated, "delete", {"draft_id": reopened_id}))["deleted"] is True
        assert bridge.call(recreated, "state", {"draft_id": reopened_id})["error"]["status"] == 404
    finally:
        for session in sessions:
            bridge.lib.seattrellis_session_destroy(session)


def ownership_and_limits(bridge: Bridge):
    session = bridge.lib.seattrellis_session_create()
    buffers = []
    try:
        draft = success(bridge.call(session, "generate", source()))["editor"]["draft_id"]
        assert bridge.decode(bridge.raw(session, b"\xff"))["error"]["code"] == "invalid_json"
        assert bridge.decode(bridge.lib.seattrellis_session_dispatch(session, None, 0))["error"]["code"] == "invalid_buffer"
        assert bridge.decode(bridge.lib.seattrellis_session_dispatch(session, None, 8 * 1024 * 1024 + 1))["error"]["code"] == "input_too_large"
        assert bridge.decode(bridge.raw(session, b'{"protocol_version":2,"operation":"state","payload":{}}'))["error"]["code"] == "protocol_mismatch"
        invalid = b'{"protocol_version":1,"operation":"unknown","payload":{}}'
        for _ in range(256):
            buffer = bridge.raw(session, invalid)
            assert bool(buffer.data)
            buffers.append(buffer)
        full = bridge.raw(session, invalid)
        assert not full.data and full.len == 0
        blocked = bridge.raw(session, json.dumps({
            "protocol_version": 1, "operation": "command", "payload": command(draft, 0, "quota-blocked", [
                {"kind": "lock_student", "payload": {"student_key": "S2"}}
            ])
        }).encode("utf-8"))
        assert not blocked.data and blocked.len == 0
        bridge.lib.seattrellis_buffer_free(buffers.pop())
        assert bridge.decode(bridge.raw(session, invalid))["error"]["code"] == "unknown_operation"
        state = success(bridge.call(session, "state", {"draft_id": draft}))
        assert state["revision"] == 0 and all(not student["locked"] for student in state["students"])
    finally:
        for buffer in buffers:
            bridge.lib.seattrellis_buffer_free(buffer)
        bridge.lib.seattrellis_session_destroy(session)


def preferences_child(library: str):
    bridge = Bridge(library)
    session = bridge.lib.seattrellis_session_create()
    try:
        draft = success(bridge.call(session, "generate", source()))["editor"]["draft_id"]
        exported = success(bridge.call(session, "export", {"draft_id": draft, "format": "svg"}))
        svg = base64.b64decode(exported["base64"], validate=True).decode("utf-8")
        assert "张同学" in svg and "李同学" in svg, "native defaults must ignore global anonymization preference"
        exported = success(bridge.call(session, "export", {
            "draft_id": draft, "format": "svg", "options": {"privacy": {"anonymize": True}}
        }))
        svg = base64.b64decode(exported["base64"], validate=True).decode("utf-8")
        assert "张同学" not in svg and "李同学" not in svg
    finally:
        bridge.lib.seattrellis_session_destroy(session)


def isolated_preferences(library: str):
    with tempfile.TemporaryDirectory(prefix="seattrellis-abi-preferences-") as temp:
        root = Path(temp)
        path = root / "existing" / "seattrellis" / "export-defaults.json"
        path.parent.mkdir(parents=True)
        original = b'{"template":"public","anonymize":true,"paper_size":"a3","locale":"en"}\n'
        path.write_bytes(original)
        for config in [root / "existing", root / "empty"]:
            environment = os.environ.copy()
            environment["XDG_CONFIG_HOME"] = str(config)
            completed = subprocess.run(
                [sys.executable, str(Path(__file__).resolve()), str(Path(library).resolve()), "--preferences-child"],
                env=environment, capture_output=True, text=True, timeout=60,
            )
            assert completed.returncode == 0, completed.stderr
        assert path.read_bytes() == original, "native export must not modify existing global preferences"
        assert not (root / "empty").exists(), "native export must not create a global preferences directory"


def main():
    if len(sys.argv) not in (2, 3):
        raise SystemExit("usage: abi_smoke.py <built-library> [--preferences-child]")
    if len(sys.argv) == 3:
        assert sys.argv[2] == "--preferences-child"
        preferences_child(sys.argv[1])
        return
    bridge = Bridge(sys.argv[1])
    lifecycle(bridge)
    ownership_and_limits(bridge)
    isolated_preferences(sys.argv[1])
    print("C ABI smoke passed: lifecycle, isolation, ownership, quotas, version, privacy and preferences")


if __name__ == "__main__":
    main()
