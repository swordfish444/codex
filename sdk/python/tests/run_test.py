from __future__ import annotations

from pathlib import Path
from typing import Callable

import pytest

from openai_codex_sdk import Codex, CodexOptions
from openai_codex_sdk.turn_options import TurnOptions

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

    spy = codex_exec_spy(proxy)

    client = Codex(CodexOptions(executable_path=str(CODEX_EXEC_PATH), base_url="http://proxy", api_key="test"))
    thread = client.start_thread()

    result = await thread.run("Hello, world!")

    expected_items = [
        {
            "id": "msg_mock",
            "item_type": "assistant_message",
            "text": "Hi!",
        }
    ]
    assert result.items == expected_items
    assert thread.id is not None


@pytest.mark.asyncio
async def test_sends_previous_items_when_run_called_twice(
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
    await thread.run("first input")
    await thread.run("second input")

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
async def test_continues_thread_with_options(
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
    await thread.run("first input")
    await thread.run("second input", TurnOptions(model="gpt-test-1"))

    assert len(proxy.requests) >= 2
    second_request = proxy.requests[1]
    payload = second_request["json"]
    assert payload.get("model") == "gpt-test-1"
    assistant_entry = next((entry for entry in payload["input"] if entry["role"] == "assistant"), None)
    assert assistant_entry is not None
    assistant_text = next(
        (item["text"] for item in assistant_entry.get("content", []) if item.get("type") == "output_text"),
        None,
    )
    assert assistant_text == "First response"


@pytest.mark.asyncio
async def test_resumes_thread_by_id(
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
    await original_thread.run("first input")

    resumed_thread = client.resume_thread(original_thread.id or "")
    result = await resumed_thread.run("second input")

    assert resumed_thread.id == original_thread.id
    assert result.final_response == "Second response"
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
async def test_passes_turn_options_to_exec(
    make_responses_proxy, codex_exec_spy: Callable[[ResponsesProxy], CodexExecSpyResult]
) -> None:
    proxy = await make_responses_proxy(
        {
            "status_code": 200,
            "response_bodies": [
                sse(
                    response_started("response_1"),
                    assistant_message("Turn options applied", "item_1"),
                    response_completed("response_1"),
                )
            ],
        }
    )

    spy = codex_exec_spy(proxy)

    client = Codex(CodexOptions(executable_path=str(CODEX_EXEC_PATH), base_url="http://proxy", api_key="test"))

    thread = client.start_thread()
    await thread.run(
        "apply options",
        TurnOptions(model="gpt-test-1", sandbox_mode="workspace-write"),
    )

    assert proxy.requests
    payload = proxy.requests[0]["json"]
    assert payload.get("model") == "gpt-test-1"

    assert spy.args
    command_args = spy.args[0]
    assert command_args.sandbox_mode == "workspace-write"
    assert command_args.model == "gpt-test-1"
