import tempfile
from pathlib import Path
import toml
import pytest

from ..tasklib import TaskMeta, load_task, save_task

SAMPLE = """+++
id = "99"
title = "Sample Task"
status = "Not started"
dependencies = ""
last_updated = "2023-01-01T12:00:00"
++

# Body here
"""

def test_load_and_save(tmp_path):
    md = tmp_path / '99-sample.md'
    md.write_text(SAMPLE)
    meta, body = load_task(md)
    assert meta.id == '99'
    assert 'Body here' in body
    meta.status = 'Done'
    save_task(md, meta, body)
    text = md.read_text()
    data = toml.loads(text.split('+++')[1])
    assert data['status'] == 'Done'

from pydantic import ValidationError

def test_meta_model_validation():
    with pytest.raises(ValidationError):
        TaskMeta(id='a', title='t', status='bogus', dependencies='', last_updated='bad')
