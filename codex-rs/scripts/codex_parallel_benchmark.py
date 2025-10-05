#!/usr/bin/env python3
"""Benchmark codex exec runs with and without parallel tool calls."""
from __future__ import annotations

import argparse
import json
import math
import os
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from concurrent.futures import Future, ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Iterable, Sequence


DEFAULT_MODEL = "gpt-5-codex"


@dataclass
class ModeConfig:
    label: str
    model: str
    extra_args: tuple[str, ...]
    env_pairs: tuple[tuple[str, str], ...]
    parallel_flag: str | None
    enabled: bool = True


@dataclass
class RunResult:
    index: int
    duration_s: float
    returncode: int
    stdout_path: Path
    stderr_path: Path
    metadata_path: Path


@dataclass
class ModeResult:
    config: ModeConfig
    outputs_dir: Path
    runs: list[RunResult]

    @property
    def durations(self) -> list[float]:
        return [run.duration_s for run in self.runs]


class BenchmarkError(RuntimeError):
    pass


@dataclass
class ProgressTracker:
    total_runs: int
    completed: int = 0
    lock: threading.Lock = field(default_factory=threading.Lock, repr=False)

    def advance(self, mode_label: str, run_index: int) -> None:
        with self.lock:
            self.completed += 1
            percentage = (self.completed / self.total_runs) * 100 if self.total_runs else 100.0
            print(
                f"[{self.completed:>3}/{self.total_runs:<3} | {percentage:5.1f}%] "
                f"mode={mode_label} run={run_index:03d}",
                flush=True,
            )


def parse_key_value_pairs(pairs: Iterable[str]) -> tuple[tuple[str, str], ...]:
    parsed: list[tuple[str, str]] = []
    for pair in pairs:
        if "=" not in pair:
            raise BenchmarkError(f"Expected KEY=VALUE format, got: {pair}")
        key, value = pair.split("=", 1)
        parsed.append((key, value))
    return tuple(parsed)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run codex exec repeatedly for parallel vs serial tool call models.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("prompt", help="Prompt passed to codex exec. Use quotes to preserve spaces.")
    parser.add_argument(
        "-n",
        "--runs",
        type=int,
        default=5,
        help="Number of executions per mode (parallel and serial).",
    )
    parser.add_argument(
        "--codex-bin",
        default="codex",
        help="Path to codex binary. If relative, resolved against the working directory.",
    )
    parser.add_argument(
        "--workdir",
        default=str(Path(__file__).resolve().parents[1]),
        help="Working directory passed to codex exec commands.",
    )
    parser.add_argument(
        "--model",
        default=DEFAULT_MODEL,
        help="Model slug shared by both modes when explicit overrides are not provided.",
    )
    parser.add_argument(
        "--parallel-model",
        default=None,
        help="Model slug used only for parallel runs; defaults to --model when omitted.",
    )
    parser.add_argument(
        "--serial-model",
        default=None,
        help="Model slug used only for serial runs; defaults to --model when omitted.",
    )
    parser.add_argument(
        "--parallel-extra",
        default="",
        help="Additional CLI args passed only to parallel runs (quoted string).",
    )
    parser.add_argument(
        "--serial-extra",
        default="",
        help="Additional CLI args passed only to serial runs (quoted string).",
    )
    parser.add_argument(
        "--parallel-env",
        action="append",
        default=[],
        help="Environment overrides KEY=VALUE applied to parallel runs (repeatable).",
    )
    parser.add_argument(
        "--serial-env",
        action="append",
        default=[],
        help="Environment overrides KEY=VALUE applied to serial runs (repeatable).",
    )
    parser.add_argument(
        "--output-root",
        default=str(Path(tempfile.gettempdir()) / "codex_parallel_benchmark"),
        help="Directory under which experiment outputs and plots are stored.",
    )
    parser.add_argument(
        "--label",
        default=datetime.now().strftime("%Y%m%d-%H%M%S"),
        help="Label used to create a unique run directory under output-root.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print summary JSON in addition to the human-readable report.",
    )
    parser.add_argument(
        "--skip-parallel",
        action="store_true",
        help="Skip runs flagged as parallel (only serial runs execute).",
    )
    parser.add_argument(
        "--skip-serial",
        action="store_true",
        help="Skip runs flagged as serial (only parallel runs execute).",
    )
    parser.add_argument(
        "--parallel-runs",
        action="store_true",
        help="Execute all codex exec runs concurrently instead of sequentially.",
    )
    parser.add_argument(
        "--max-workers",
        type=int,
        default=None,
        help="Maximum number of in-flight codex exec runs when --parallel-runs is set.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the commands that would run without executing them.",
    )
    return parser.parse_args(argv[1:])


def ensure_binary(path: str) -> str:
    candidate = Path(path)
    if candidate.is_file():
        return str(candidate.resolve())
    resolved = shutil.which(path)
    if not resolved:
        raise BenchmarkError(f"Unable to locate codex binary: {path}")
    return resolved


def expand_args(arg_string: str) -> tuple[str, ...]:
    if not arg_string.strip():
        return tuple()
    return tuple(shlex.split(arg_string))


def build_mode_configs(args: argparse.Namespace) -> list[ModeConfig]:
    parallel_model = args.parallel_model or args.model
    serial_model = args.serial_model or args.model

    modes = [
        ModeConfig(
            label="parallel_on",
            model=parallel_model,
            extra_args=expand_args(args.parallel_extra),
            env_pairs=parse_key_value_pairs(args.parallel_env),
            parallel_flag="on",
            enabled=not args.skip_parallel,
        ),
        ModeConfig(
            label="parallel_off",
            model=serial_model,
            extra_args=expand_args(args.serial_extra),
            env_pairs=parse_key_value_pairs(args.serial_env),
            parallel_flag="off",
            enabled=not args.skip_serial,
        ),
    ]
    enabled_modes = [mode for mode in modes if mode.enabled]
    if not enabled_modes:
        raise BenchmarkError("All modes skipped; enable at least one mode to run the benchmark.")
    return enabled_modes


def run_command(
    codex_bin: str,
    workdir: Path,
    prompt: str,
    mode: ModeConfig,
    run_index: int,
    output_dir: Path,
    dry_run: bool,
) -> RunResult:
    workdir.mkdir(parents=True, exist_ok=True)
    mode_dir = output_dir / mode.label
    run_dir = mode_dir / f"run_{run_index:03d}"
    run_dir.mkdir(parents=True, exist_ok=True)

    command = [codex_bin, "exec", "--model", mode.model]
    if mode.parallel_flag:
        command.extend(["--parallel-tool-calls", mode.parallel_flag])
    command.extend((*mode.extra_args, prompt))
    env = os.environ.copy()
    for key, value in mode.env_pairs:
        env[key] = value

    stdout_path = run_dir / "stdout.txt"
    stderr_path = run_dir / "stderr.txt"
    metadata_path = run_dir / "metadata.json"

    start_dt = datetime.now()
    if dry_run:
        duration_s = float("nan")
        returncode = 0
        stdout = ""
        stderr = ""
    else:
        start = time.perf_counter()
        result = subprocess.run(
            command,
            cwd=str(workdir),
            capture_output=True,
            text=True,
            env=env,
            check=False,
        )
        duration_s = time.perf_counter() - start
        returncode = result.returncode
        stdout = result.stdout
        stderr = result.stderr
        stdout_path.write_text(stdout)
        stderr_path.write_text(stderr)

    metadata = {
        "command": command,
        "env_overrides": {key: value for key, value in mode.env_pairs},
        "model": mode.model,
        "label": mode.label,
        "prompt": prompt,
        "run_index": run_index,
        "duration_seconds": duration_s,
        "returncode": returncode,
        "started_at": start_dt.isoformat(),
    }
    metadata_path.write_text(json.dumps(metadata, indent=2))

    if dry_run:
        command_str = " ".join(shlex.quote(element) for element in command)
        print(f"[DRY-RUN] {command_str}")

    return RunResult(
        index=run_index,
        duration_s=duration_s,
        returncode=returncode,
        stdout_path=stdout_path,
        stderr_path=stderr_path,
        metadata_path=metadata_path,
    )


def execute_runs(
    *,
    codex_bin: str,
    workdir: Path,
    prompt: str,
    modes: Sequence[ModeConfig],
    runs_per_mode: int,
    output_dir: Path,
    dry_run: bool,
    progress: ProgressTracker,
    parallel_runs: bool,
    max_workers: int | None,
) -> list[ModeResult]:
    if not modes:
        return []
    if parallel_runs:
        total_runs = runs_per_mode * len(modes)
        worker_count = max_workers or total_runs
        if worker_count < 1:
            raise BenchmarkError("max workers must be a positive integer")
        runs_by_mode: dict[str, list[RunResult]] = {mode.label: [] for mode in modes}
        future_to_mode: dict[Future[RunResult], tuple[ModeConfig, int]] = {}
        with ThreadPoolExecutor(max_workers=worker_count) as executor:
            for mode in modes:
                for idx in range(1, runs_per_mode + 1):
                    future = executor.submit(
                        run_command,
                        codex_bin,
                        workdir,
                        prompt,
                        mode,
                        idx,
                        output_dir,
                        dry_run,
                    )
                    future_to_mode[future] = (mode, idx)
            for future in as_completed(future_to_mode):
                mode, _ = future_to_mode[future]
                result = future.result()
                runs_by_mode[mode.label].append(result)
                progress.advance(mode.label, result.index)
        mode_results: list[ModeResult] = []
        for mode in modes:
            runs = sorted(runs_by_mode[mode.label], key=lambda run: run.index)
            mode_results.append(ModeResult(config=mode, outputs_dir=output_dir / mode.label, runs=runs))
        return mode_results

    mode_results = []
    for mode in modes:
        runs: list[RunResult] = []
        for idx in range(1, runs_per_mode + 1):
            result = run_command(
                codex_bin=codex_bin,
                workdir=workdir,
                prompt=prompt,
                mode=mode,
                run_index=idx,
                output_dir=output_dir,
                dry_run=dry_run,
            )
            runs.append(result)
            progress.advance(mode.label, idx)
        mode_results.append(ModeResult(config=mode, outputs_dir=output_dir / mode.label, runs=runs))
    return mode_results


def compute_stats(values: Sequence[float]) -> dict[str, float | int]:
    clean_values = [value for value in values if math.isfinite(value)]
    if not clean_values:
        return {"count": 0}
    stats: dict[str, float | int] = {
        "count": len(clean_values),
        "min": min(clean_values),
        "max": max(clean_values),
        "mean": statistics.mean(clean_values),
        "median": statistics.median(clean_values),
    }
    if len(clean_values) > 1:
        stats["stdev"] = statistics.stdev(clean_values)
    return stats


def summarize(mode_results: list[ModeResult]) -> dict[str, dict[str, float | int]]:
    summary: dict[str, dict[str, float | int]] = {}
    for result in mode_results:
        summary[result.config.label] = compute_stats(result.durations)
    return summary


def write_summary(
    output_dir: Path,
    summary: dict[str, dict[str, float | int]],
    mode_results: list[ModeResult],
) -> Path:
    payload = {
        "output_dir": str(output_dir),
        "summary": summary,
        "runs": {
            result.config.label: [
                {
                    "index": run.index,
                    "duration_seconds": run.duration_s,
                    "returncode": run.returncode,
                    "stdout_path": str(run.stdout_path),
                    "stderr_path": str(run.stderr_path),
                }
                for run in result.runs
            ]
            for result in mode_results
        },
    }
    summary_path = output_dir / "summary.json"
    summary_path.write_text(json.dumps(payload, indent=2))
    return summary_path


def attempt_plot(output_dir: Path, mode_results: list[ModeResult]) -> Path | None:
    has_finite = any(
        math.isfinite(duration)
        for result in mode_results
        for duration in result.durations
    )
    if not has_finite:
        return None
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except Exception as exc:  # pragma: no cover - plotting is optional
        print(f"[WARN] Unable to create plot ({exc}); continue without chart.")
        return None

    fig, ax = plt.subplots(figsize=(8, 4))
    labels = [result.config.label for result in mode_results]
    data = [
        [value for value in result.durations if math.isfinite(value)]
        for result in mode_results
    ]
    ax.boxplot(data, labels=labels, showmeans=True)
    ax.set_ylabel("Duration (seconds)")
    ax.set_title("codex exec durations by mode")
    ax.grid(True, axis="y", linestyle="--", alpha=0.4)
    plot_path = output_dir / "duration_boxplot.png"
    fig.tight_layout()
    fig.savefig(plot_path)
    plt.close(fig)
    return plot_path


def format_report(summary: dict[str, dict[str, float | int]], output_dir: Path, plot_path: Path | None) -> str:
    lines = ["Benchmark summary:"]
    for label, stats in summary.items():
        lines.append(f"  {label}:")
        for key in sorted(stats):
            value = stats[key]
            if isinstance(value, float):
                lines.append(f"    {key}: {value:.4f}")
            else:
                lines.append(f"    {key}: {value}")
    lines.append(f"Outputs stored in: {output_dir}")
    if plot_path:
        lines.append(f"Plot saved to: {plot_path}")
    return "\n".join(lines)


def main(argv: Sequence[str]) -> int:
    args = parse_args(argv)
    try:
        codex_bin = ensure_binary(args.codex_bin)
        workdir = Path(args.workdir).resolve()
        output_root = Path(args.output_root).resolve()
        run_dir = output_root / args.label
        run_dir.mkdir(parents=True, exist_ok=True)
        modes = build_mode_configs(args)

        total_runs = len(modes) * args.runs
        if args.max_workers is not None and args.max_workers < 1:
            raise BenchmarkError("--max-workers must be a positive integer")
        progress = ProgressTracker(total_runs=total_runs)
        parallel_runs = args.parallel_runs
        mode_results = execute_runs(
            codex_bin=codex_bin,
            workdir=workdir,
            prompt=args.prompt,
            modes=modes,
            runs_per_mode=args.runs,
            output_dir=run_dir,
            dry_run=args.dry_run,
            progress=progress,
            parallel_runs=parallel_runs,
            max_workers=args.max_workers,
        )

        summary = summarize(mode_results)
        summary_path = write_summary(run_dir, summary, mode_results)
        plot_path = attempt_plot(run_dir, mode_results)
        report = format_report(summary, run_dir, plot_path)
        print(report)
        if args.json:
            payload = {
                "summary": summary,
                "output_dir": str(run_dir),
                "plot_path": str(plot_path) if plot_path else None,
                "summary_path": str(summary_path),
            }
            print(json.dumps(payload, indent=2))
        return 0
    except BenchmarkError as error:
        print(f"[ERROR] {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
