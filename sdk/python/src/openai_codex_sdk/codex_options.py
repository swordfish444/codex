from __future__ import annotations

from dataclasses import dataclass


@dataclass(slots=True)
class CodexOptions:
    """Configuration for creating a ``Codex`` client."""

    executable_path: str
    base_url: str | None = None
    api_key: str | None = None
