"""Storage utilities for conversations using boostedblob."""

from __future__ import annotations

import json
from datetime import datetime
from typing import Any, List

import boostedblob
from pydantic import BaseModel, Field


CONVERSATIONS_PREFIX = "az://whistlerdata/codirect/conversations"


class ConversationMetadata(BaseModel):
    """Metadata about a stored conversation.

    This includes several dummy fields for demonstration purposes.
    """

    conversation_id: str
    title: str
    user_id: str
    created_at: datetime
    language: str = "en"
    tags: List[str] = Field(default_factory=list)
    is_favorite: bool = False
    model_name: str | None = None


def _conversation_path(conversation_id: str) -> str:
    return f"{CONVERSATIONS_PREFIX}/{conversation_id}"


def write_conversation(
    conversation_id: str,
    metadata: ConversationMetadata,
    conversation: Any,
) -> None:
    """Write a conversation and its metadata to object storage.

    Args:
        conversation_id: Unique identifier for the conversation.
        metadata: Metadata describing the conversation.
        conversation: The conversation payload (must be JSON serializable).
    """

    base = _conversation_path(conversation_id)
    with boostedblob.open(f"{base}/metadata.json", "w") as f:
        f.write(metadata.model_dump_json(indent=2))

    with boostedblob.open(f"{base}/conversation.json", "w") as f:
        json.dump(conversation, f, indent=2)


def list_conversations() -> List[str]:
    """List all conversation IDs under the conversations prefix."""

    entries = boostedblob.listdir(CONVERSATIONS_PREFIX)
    ids = []
    for entry in entries:
        name = entry.rstrip("/").split("/")[-1]
        if name:
            ids.append(name)
    return ids
