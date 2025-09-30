from __future__ import annotations

from .codex_options import CodexOptions
from .exec import CodexExec
from .thread import Thread


class Codex:
    def __init__(self, options: CodexOptions) -> None:
        if not options.executable_path:
            raise ValueError("executable_path is required")

        self._exec = CodexExec(options.executable_path)
        self._options = options

    def start_thread(self) -> Thread:
        return Thread(self._exec, self._options)

    def resume_thread(self, thread_id: str) -> Thread:
        return Thread(self._exec, self._options, thread_id)
