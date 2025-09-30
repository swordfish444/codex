# openai-codex-sdk

A modern, minimalistic Python library project scaffold.

## Features

- PEP 621 `pyproject.toml` with `hatchling` build backend
- `src/` layout for package code
- Preconfigured tooling: Ruff, MyPy, and Pytest
- Ready for publishing to PyPI and local development

## Getting Started

```bash
python -m venv .venv
source .venv/bin/activate
pip install -U pip
pip install -e .[dev]
```

## Running Tests

```bash
pytest
```

## Linting & Formatting

```bash
ruff check src tests
ruff format src tests
mypy src
```

## Releasing

Update the version in `src/openai_codex_sdk/__about__.py` and `pyproject.toml`, then build and publish:

```bash
rm -rf dist
python -m build
python -m twine upload dist/*
```
