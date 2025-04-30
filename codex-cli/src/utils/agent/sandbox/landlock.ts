import type { ExecResult } from "./interface.js";
import type { SpawnOptions } from "child_process";

import { exec } from "./raw-exec.js";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

export async function execWithLandlock(
  cmd: Array<string>,
  opts: SpawnOptions,
  userProvidedWritableRoots: ReadonlyArray<string>,
  abortSignal?: AbortSignal,
): Promise<ExecResult> {
  const sandboxExecutable = await getSandboxExecutable();

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

/**
 * Lazily initialized promise that resolves to the absolute path of the
 * architecture-specific Landlock helper binary.
 */
let sandboxExecutablePromise: Promise<string> | null = null;

async function detectSandboxExecutable(): Promise<string> {
  // Map Node-reported architectures to the corresponding binary name.
  const exeBaseName: string = (() => {
    switch (process.arch) {
      case "arm64":
        return "codex-linux-sandbox-arm64";
      case "x64":
        return "codex-linux-sandbox-x64";
      // Fall back to the x86_64 build for anything else – it will obviously
      // fail on incompatible systems but gives a sane error message rather
      // than crashing earlier.
      default:
        return "codex-linux-sandbox-x64";
    }
  })();

  // Find the executable relative to the package.json file.
  const __filename = fileURLToPath(import.meta.url);
  let dir: string = path.dirname(__filename);

  // Ascend until package.json is found or we reach the filesystem root.
  // eslint-disable-next-line no-constant-condition
  while (true) {
    try {
      // eslint-disable-next-line no-await-in-loop
      await fs.promises.access(
        path.join(dir, "package.json"),
        fs.constants.F_OK,
      );
      break; // Found the package.json ⇒ dir is our project root.
    } catch {
      // keep searching
    }

    const parent = path.dirname(dir);
    if (parent === dir) {
      throw new Error("Unable to locate package.json");
    }
    dir = parent;
  }

  const candidate = path.join(dir, "bin", exeBaseName);
  try {
    await fs.promises.access(candidate, fs.constants.X_OK);
    return candidate;
  } catch {
    throw new Error(`${candidate} not found or not executable`);
  }
}

/**
 * Returns the absolute path to the architecture-specific Landlock helper
 * binary. (Could be a rejected promise if not found.)
 */
function getSandboxExecutable(): Promise<string> {
  if (!sandboxExecutablePromise) {
    sandboxExecutablePromise = detectSandboxExecutable();
  }

  return sandboxExecutablePromise;
}
