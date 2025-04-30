import type { ExecResult } from "./sandbox/interface";
import type { SpawnOptions } from "child_process";

import { exec } from "./raw-exec.js";

export function execWithLandlock(
  cmd: Array<string>,
  opts: SpawnOptions,
  userProvidedWritableRoots: ReadonlyArray<string>,
  abortSignal?: AbortSignal,
): Promise<ExecResult> {
  // TODO(mbolin): Find the arch-appropriate sandbox executable.
  const sandboxExecutable = "bin/codex-linux-sandbox-arm64";

  const extraSandboxPermissions = userProvidedWritableRoots.flatMap(
    (root: string) => ["--sandbox-permission", `disk-write-folder=${root}`],
  );
  const fullCommand = [
    sandboxExecutable,
    "--full-auto",

    "--sandbox-permission",
    "disk-full-read-access",

    "--sandbox-permission",
    "disk-write-cwd",

    "--sandbox-permission",
    "disk-write-platform-user-temp-folder",

    ...extraSandboxPermissions,

    "--",
    ...cmd,
  ];

  return exec(fullCommand, opts, abortSignal);
}
