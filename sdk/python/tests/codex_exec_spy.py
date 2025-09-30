from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

from pytest import MonkeyPatch

from openai_codex_sdk.exec import CodexExecArgs

from .responses_proxy import FakeExec, ResponsesProxy


@dataclass(slots=True)
class CodexExecSpyResult:
    args: list[CodexExecArgs]
    restore: Callable[[], None]


def install_codex_exec_spy(monkeypatch: MonkeyPatch, proxy: ResponsesProxy) -> CodexExecSpyResult:
    calls: list[CodexExecArgs] = []

    def factory(path: str) -> FakeExec:
        return FakeExec(path, proxy, calls)

    monkeypatch.setattr("openai_codex_sdk.codex.CodexExec", factory)

    return CodexExecSpyResult(args=calls, restore=monkeypatch.undo)
