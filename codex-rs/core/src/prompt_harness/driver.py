#!/usr/bin/env python3
import json
import sys
from typing import Iterator


def render_agent_message(message: object) -> str:
    if isinstance(message, str):
        return message
    if isinstance(message, dict) and "content" in message:
        return json.dumps(message)
    return str(message)


def send_question(questions: Iterator[str], turn: int) -> bool:
    try:
        text = next(questions)
    except StopIteration:
        return False

    payload = {
        "id": f"turn-{turn}",
        "op": {
            "type": "user_input",
            "items": [
                {
                    "type": "text",
                    "text": text,
                }
            ],
        },
    }
    print(json.dumps(payload), flush=True)
    print(f"[user] {text}", file=sys.stderr)
    return True


def main() -> None:
    questions = iter(["What is your name?", "1+1=?"])
    turn = 1

    for raw in sys.stdin:
        event = json.loads(raw)
        kind = event.get("msg", {}).get("type")

        if kind != "agent_message_delta" and kind != "agent_reasoning_delta":
            print(f"[harness] event {kind}", file=sys.stderr)

        if kind == "session_configured":
            if send_question(questions, turn):
                continue
        elif kind == "user_message":
            print(f"[user_message raw] {json.dumps(event)}", file=sys.stderr)
        elif kind == "agent_message":
            message = event.get("msg", {}).get("message")
            print(f"[agent] {render_agent_message(message)}", file=sys.stderr)
        elif kind == "task_complete":
            turn += 1
            if not send_question(questions, turn):
                break


if __name__ == "__main__":
    main()
