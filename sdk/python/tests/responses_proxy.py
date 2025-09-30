from __future__ import annotations

import asyncio
import itertools
import json
from dataclasses import dataclass, field
from typing import Any, AsyncGenerator, TypedDict

from openai_codex_sdk.exec import CodexExecArgs

DEFAULT_RESPONSE_ID = "resp_mock"
DEFAULT_MESSAGE_ID = "msg_mock"


class SseEvent(TypedDict, total=False):
    type: str
    item: dict[str, Any]
    response: dict[str, Any]


class SseResponseBody(TypedDict):
    kind: str
    events: list[SseEvent]


class ResponsesProxyOptions(TypedDict, total=False):
    response_bodies: list[SseResponseBody]
    status_code: int


class RecordedRequest(TypedDict):
    body: str
    json: dict[str, Any]


@dataclass(slots=True)
class ResponsesProxy:
    response_bodies: list[SseResponseBody]
    status_code: int
    requests: list[RecordedRequest]
    _response_index: int = field(init=False, default=0)
    _thread_counter: itertools.count = field(init=False, default_factory=lambda: itertools.count(1))
    _thread_histories: dict[str, list[str]] = field(init=False, default_factory=dict)

    def __post_init__(self) -> None:
        if not self.response_bodies:
            raise ValueError("response_bodies is required")

    async def close(self) -> None:
        await asyncio.sleep(0)

    def _next_thread_id(self) -> str:
        return f"thread_{next(self._thread_counter)}"

    def _next_response(self) -> SseResponseBody:
        index = min(self._response_index, len(self.response_bodies) - 1)
        self._response_index += 1
        return self.response_bodies[index]

    def _build_request(self, args: CodexExecArgs, thread_id: str) -> RecordedRequest:
        history = self._thread_histories.get(thread_id, [])
        input_entries: list[dict[str, Any]] = []
        for text in history:
            input_entries.append(
                {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "output_text",
                            "text": text,
                        }
                    ],
                }
            )
        input_entries.append(
            {
                "role": "user",
                "content": [
                    {
                        "type": "input_text",
                        "text": args.input,
                    }
                ],
            }
        )

        request_json: dict[str, Any] = {"input": input_entries}
        if args.model is not None:
            request_json["model"] = args.model

        recorded = RecordedRequest(body=json.dumps(request_json), json=request_json)
        self.requests.append(recorded)
        return recorded

    def record_run(self, args: CodexExecArgs) -> tuple[str, RecordedRequest, bool]:
        if args.thread_id:
            thread_id = args.thread_id
            new_thread = False
        else:
            thread_id = self._next_thread_id()
            new_thread = True
        request = self._build_request(args, thread_id)
        return thread_id, request, new_thread

    def add_history(self, thread_id: str, text: str) -> None:
        self._thread_histories.setdefault(thread_id, []).append(text)

    def _convert_events(
        self, response_body: SseResponseBody, thread_id: str, new_thread: bool
    ) -> list[dict[str, Any]]:
        events: list[dict[str, Any]] = []
        if new_thread:
            events.append({"type": "thread.started", "thread_id": thread_id})

        for event in response_body["events"]:
            if event["type"] == "response.created":
                events.append({"type": "turn.started"})
            elif event["type"] == "response.output_item.done":
                item = event["item"]
                text = item["content"][0]["text"]
                events.append(
                    {
                        "type": "item.completed",
                        "item": {
                            "id": item["id"],
                            "item_type": "assistant_message",
                            "text": text,
                        },
                    }
                )
            elif event["type"] == "response.completed":
                events.append(
                    {
                        "type": "turn.completed",
                        "usage": {
                            "input_tokens": 0,
                            "cached_input_tokens": 0,
                            "output_tokens": 0,
                        },
                    }
                )
        return events

    def next_events(self, thread_id: str, new_thread: bool) -> list[dict[str, Any]]:
        response_body = self._next_response()
        return self._convert_events(response_body, thread_id, new_thread)


class FakeExec:
    def __init__(self, _path: str, proxy: ResponsesProxy, calls: list[CodexExecArgs]) -> None:
        self._proxy = proxy
        self.calls = calls

    async def run(self, args: CodexExecArgs) -> AsyncGenerator[str, None]:
        self.calls.append(args)
        thread_id, _request, new_thread = self._proxy.record_run(args)
        events = self._proxy.next_events(thread_id, new_thread)

        for event in events:
            if event["type"] == "item.completed":
                item = event["item"]
                text = item.get("text")
                if text:
                    self._proxy.add_history(thread_id, text)
            await asyncio.sleep(0)
            yield json.dumps(event)


async def start_responses_test_proxy(options: ResponsesProxyOptions) -> ResponsesProxy:
    response_bodies = options.get("response_bodies")
    if response_bodies is None:
        raise ValueError("response_bodies is required")
    status_code = options.get("status_code", 200)
    proxy = ResponsesProxy(response_bodies, status_code, requests=[])
    return proxy


def sse(*events: SseEvent) -> SseResponseBody:
    return {"kind": "sse", "events": list(events)}


def response_started(response_id: str = DEFAULT_RESPONSE_ID) -> SseEvent:
    return {
        "type": "response.created",
        "response": {"id": response_id},
    }


def assistant_message(text: str, item_id: str = DEFAULT_MESSAGE_ID) -> SseEvent:
    return {
        "type": "response.output_item.done",
        "item": {
            "type": "message",
            "role": "assistant",
            "id": item_id,
            "content": [
                {
                    "type": "output_text",
                    "text": text,
                }
            ],
        },
    }


def response_completed(response_id: str = DEFAULT_RESPONSE_ID) -> SseEvent:
    return {
        "type": "response.completed",
        "response": {"id": response_id},
    }
