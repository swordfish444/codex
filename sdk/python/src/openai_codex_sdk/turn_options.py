from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

ApprovalMode = Literal["never", "on-request", "on-failure", "untrusted"]
SandboxMode = Literal["read-only", "workspace-write", "danger-full-access"]


@dataclass(slots=True)
class TurnOptions:
    model: str | None = None
    sandbox_mode: SandboxMode | None = None
