from __future__ import annotations

import json
from dataclasses import dataclass
from typing import AsyncGenerator, cast

from .codex_options import CodexOptions
from .exec import CodexExec, CodexExecArgs
from .events import ItemCompletedEvent, ThreadEvent, ThreadStartedEvent
from .items import AssistantMessageItem, ThreadItem
from .turn_options import TurnOptions

Input = str


@dataclass(slots=True)
class RunResult:
    items: list[ThreadItem]
    final_response: str


@dataclass(slots=True)
class RunStreamedResult:
    events: AsyncGenerator[ThreadEvent, None]


class Thread:
    def __init__(self, codex_exec: CodexExec, options: CodexOptions, thread_id: str | None = None) -> None:
        self._exec = codex_exec
        self._options = options
        self.id = thread_id

    async def run_streamed(self, input: Input, options: TurnOptions | None = None) -> RunStreamedResult:
        return RunStreamedResult(events=self._run_streamed_internal(input, options))

    async def run(self, input: Input, options: TurnOptions | None = None) -> RunResult:
        generator = self._run_streamed_internal(input, options)
        items: list[ThreadItem] = []
        final_response = ""

        async for event in generator:
            if event["type"] != "item.completed":
                continue
            completed = cast(ItemCompletedEvent, event)
            item = completed["item"]
            items.append(item)
            if item["item_type"] == "assistant_message":
                assistant_item = cast(AssistantMessageItem, item)
                final_response = assistant_item["text"]

        return RunResult(items=items, final_response=final_response)

    async def _run_streamed_internal(
        self, input: Input, options: TurnOptions | None
    ) -> AsyncGenerator[ThreadEvent, None]:
        exec_args = CodexExecArgs(
            input=input,
            base_url=self._options.base_url,
            api_key=self._options.api_key,
            thread_id=self.id,
            model=options.model if options else None,
            sandbox_mode=options.sandbox_mode if options else None,
        )

        async for raw_event in self._exec.run(exec_args):
            parsed = cast(ThreadEvent, json.loads(raw_event))
            if parsed["type"] == "thread.started":
                started = cast(ThreadStartedEvent, parsed)
                self.id = started["thread_id"]
            yield parsed
