from __future__ import annotations

import asyncio
import os
from dataclasses import dataclass
from typing import AsyncGenerator

from .turn_options import SandboxMode


@dataclass(slots=True)
class CodexExecArgs:
    input: str
    base_url: str | None = None
    api_key: str | None = None
    thread_id: str | None = None
    model: str | None = None
    sandbox_mode: SandboxMode | None = None


class CodexExec:
    def __init__(self, executable_path: str) -> None:
        self._executable_path = executable_path

    async def run(self, args: CodexExecArgs) -> AsyncGenerator[str, None]:
        command_args: list[str] = ["exec", "--experimental-json"]

        if args.model:
            command_args.extend(["--model", args.model])

        if args.sandbox_mode:
            command_args.extend(["--sandbox", args.sandbox_mode])

        if args.thread_id:
            command_args.extend(["resume", args.thread_id, args.input])
        else:
            command_args.append(args.input)

        env = dict(os.environ)
        if args.base_url:
            env["OPENAI_BASE_URL"] = args.base_url
        if args.api_key:
            env["OPENAI_API_KEY"] = args.api_key

        try:
            process = await asyncio.create_subprocess_exec(
                self._executable_path,
                *command_args,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                env=env,
            )
        except Exception as exc:  # pragma: no cover - passthrough for caller
            raise RuntimeError("Failed to start codex executable") from exc

        if not process.stdout:
            process.kill()
            await process.wait()
            raise RuntimeError("Child process has no stdout")

        try:
            while True:
                line = await process.stdout.readline()
                if not line:
                    break
                yield line.decode("utf-8").rstrip("\n")

            return_code = await process.wait()
            if return_code != 0:
                stderr_output = b""
                if process.stderr:
                    stderr_output = await process.stderr.read()
                message = stderr_output.decode("utf-8", errors="ignore").strip()
                raise RuntimeError(
                    f"Codex Exec exited with code {return_code}" + (f": {message}" if message else "")
                )
        finally:
            if process.returncode is None:
                process.kill()
                await process.wait()
