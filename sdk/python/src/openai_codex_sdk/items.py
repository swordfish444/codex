from __future__ import annotations

from typing import Literal, NotRequired, TypedDict


class CommandExecutionItem(TypedDict):
    id: str
    item_type: Literal["command_execution"]
    command: str
    aggregated_output: str
    status: Literal["in_progress", "completed", "failed"]
    exit_code: NotRequired[int]


class FileUpdateChange(TypedDict):
    path: str
    kind: Literal["add", "delete", "update"]


class FileChangeItem(TypedDict):
    id: str
    item_type: Literal["file_change"]
    changes: list[FileUpdateChange]
    status: Literal["completed", "failed"]


class McpToolCallItem(TypedDict):
    id: str
    item_type: Literal["mcp_tool_call"]
    server: str
    tool: str
    status: Literal["in_progress", "completed", "failed"]


class AssistantMessageItem(TypedDict):
    id: str
    item_type: Literal["assistant_message"]
    text: str


class ReasoningItem(TypedDict):
    id: str
    item_type: Literal["reasoning"]
    text: str


class WebSearchItem(TypedDict):
    id: str
    item_type: Literal["web_search"]
    query: str


class ErrorItem(TypedDict):
    id: str
    item_type: Literal["error"]
    message: str


class TodoItem(TypedDict):
    text: str
    completed: bool


class TodoListItem(TypedDict):
    id: str
    item_type: Literal["todo_list"]
    items: list[TodoItem]


class SessionItem(TypedDict):
    id: str
    item_type: Literal["session"]
    session_id: str


ThreadItem = (
    AssistantMessageItem
    | ReasoningItem
    | CommandExecutionItem
    | FileChangeItem
    | McpToolCallItem
    | WebSearchItem
    | TodoListItem
    | ErrorItem
)
