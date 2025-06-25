"""
Simple library for loading and saving task metadata embedded as TOML front-matter
in task Markdown files.
"""
import re
import subprocess
from datetime import datetime
from pathlib import Path

import toml
from pydantic import BaseModel, Field

FRONTMATTER_RE = re.compile(r"^\+\+\+\s*(.*?)\s*\+\+\+", re.S | re.M)

def repo_root():
    return Path(subprocess.check_output(['git', 'rev-parse', '--show-toplevel']).decode().strip())

def task_dir():
    return repo_root() / "agentydragon/tasks"

class TaskMeta(BaseModel):
    id: str
    title: str
    status: str
    dependencies: str = Field(default="")
    last_updated: datetime = Field(default_factory=datetime.utcnow)

def load_task(path: Path) -> (TaskMeta, str):
    text = path.read_text(encoding='utf-8')
    m = FRONTMATTER_RE.match(text)
    if not m:
        raise ValueError(f"No TOML frontmatter in {path}")
    meta = toml.loads(m.group(1))
    tm = TaskMeta(**meta)
    body = text[m.end():].lstrip('\n')
    return tm, body

def save_task(path: Path, meta: TaskMeta, body: str) -> None:
    tm = meta.dict()
    tm['last_updated'] = meta.last_updated.isoformat()
    fm = toml.dumps(tm).strip()
    content = f"+++\n{fm}\n+++\n\n{body.lstrip()}"
    path.write_text(content, encoding='utf-8')
