from __future__ import annotations

from pathlib import Path
from typing import AsyncGenerator, Callable

import pytest

from openai_codex_sdk import Codex, CodexOptions
from openai_codex_sdk.events import ThreadEvent

from .codex_exec_spy import CodexExecSpyResult
from .responses_proxy import (
    ResponsesProxy,
    assistant_message,
    response_completed,
    response_started,
    sse,
)

CODEX_EXEC_PATH = Path(__file__).resolve().parents[2] / "codex-rs" / "target" / "debug" / "codex"


@pytest.mark.asyncio
async def test_returns_thread_events(
    make_responses_proxy, codex_exec_spy: Callable[[ResponsesProxy], CodexExecSpyResult]
) -> None:
    proxy = await make_responses_proxy(
        {
            "status_code": 200,
            "response_bodies": [
                sse(
                    response_started(),
                    assistant_message("Hi!"),
                    response_completed(),
                )
            ],
        }
    )

    codex_exec_spy(proxy)

    client = Codex(CodexOptions(executable_path=str(CODEX_EXEC_PATH), base_url="http://proxy", api_key="test"))

    thread = client.start_thread()
    result = await thread.run_streamed("Hello, world!")

    events: list[ThreadEvent] = []
    async for event in result.events:
        events.append(event)

    assert events == [
        {
            "type": "thread.started",
            "thread_id": "thread_1",
        },
        {"type": "turn.started"},
        {
            "type": "item.completed",
            "item": {
                "id": "msg_mock",
                "item_type": "assistant_message",
                "text": "Hi!",
            },
        },
        {
            "type": "turn.completed",
            "usage": {
                "input_tokens": 0,
                "cached_input_tokens": 0,
                "output_tokens": 0,
            },
        },
    ]
    assert thread.id == "thread_1"


@pytest.mark.asyncio
async def test_sends_previous_items_when_run_streamed_called_twice(
    make_responses_proxy, codex_exec_spy: Callable[[ResponsesProxy], CodexExecSpyResult]
) -> None:
    proxy = await make_responses_proxy(
        {
            "status_code": 200,
            "response_bodies": [
                sse(
                    response_started("response_1"),
                    assistant_message("First response", "item_1"),
                    response_completed("response_1"),
                ),
                sse(
                    response_started("response_2"),
                    assistant_message("Second response", "item_2"),
                    response_completed("response_2"),
                ),
            ],
        }
    )

    codex_exec_spy(proxy)

    client = Codex(CodexOptions(executable_path=str(CODEX_EXEC_PATH), base_url="http://proxy", api_key="test"))

    thread = client.start_thread()
    first = await thread.run_streamed("first input")
    await _drain_events(first.events)

    second = await thread.run_streamed("second input")
    await _drain_events(second.events)

    assert len(proxy.requests) >= 2
    second_request = proxy.requests[1]
    payload = second_request["json"]
    assistant_entry = next((entry for entry in payload["input"] if entry["role"] == "assistant"), None)
    assert assistant_entry is not None
    assistant_text = next(
        (item["text"] for item in assistant_entry.get("content", []) if item.get("type") == "output_text"),
        None,
    )
    assert assistant_text == "First response"


@pytest.mark.asyncio
async def test_resumes_thread_by_id_when_streaming(
    make_responses_proxy, codex_exec_spy: Callable[[ResponsesProxy], CodexExecSpyResult]
) -> None:
    proxy = await make_responses_proxy(
        {
            "status_code": 200,
            "response_bodies": [
                sse(
                    response_started("response_1"),
                    assistant_message("First response", "item_1"),
                    response_completed("response_1"),
                ),
                sse(
                    response_started("response_2"),
                    assistant_message("Second response", "item_2"),
                    response_completed("response_2"),
                ),
            ],
        }
    )

    codex_exec_spy(proxy)

    client = Codex(CodexOptions(executable_path=str(CODEX_EXEC_PATH), base_url="http://proxy", api_key="test"))

    original_thread = client.start_thread()
    first = await original_thread.run_streamed("first input")
    await _drain_events(first.events)

    resumed_thread = client.resume_thread(original_thread.id or "")
    second = await resumed_thread.run_streamed("second input")
    await _drain_events(second.events)

    assert resumed_thread.id == original_thread.id

    assert len(proxy.requests) >= 2
    second_request = proxy.requests[1]
    payload = second_request["json"]
    assistant_entry = next((entry for entry in payload["input"] if entry["role"] == "assistant"), None)
    assert assistant_entry is not None
    assistant_text = next(
        (item["text"] for item in assistant_entry.get("content", []) if item.get("type") == "output_text"),
        None,
    )
    assert assistant_text == "First response"


async def _drain_events(events: AsyncGenerator[ThreadEvent, None]) -> None:
    async for _ in events:
        pass
