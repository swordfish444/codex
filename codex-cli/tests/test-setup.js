// Vitest setup file – executed in every test worker before any individual test
// suites are imported.  Node.js disallows `process.chdir()` inside worker
// threads starting from v22 which causes tests that attempt to change the
// current working directory to throw `ERR_WORKER_CANNOT_CHANGE_CWD` when the
// Vitest pool strategy spawns multiple threads.  In the real CLI this
// restriction does not apply (the program runs on the main thread), so we
// polyfill the call here to keep the behaviour consistent across execution
// environments.

import path from "node:path";

// Cache the initial CWD so we can emulate subsequent changes.
let currentCwd = process.cwd();

// Replace `process.chdir` with a version that *simulates* the directory change
// instead of delegating to Node’s native implementation when running inside a
// worker.  The polyfill updates `process.cwd()` and the `PWD` environment
// variable so that code relying on either continues to work as expected.

// eslint-disable-next-line no-global-assign, @typescript-eslint/ban-ts-comment
// @ts-ignore – Node’s types mark `process` as `Readonly<Process>` but runtime
// mutation is perfectly fine.
process.chdir = function mockedChdir(targetDir) {
  // Resolve the new directory against the current working directory just like
  // the real implementation would.
  currentCwd = path.resolve(currentCwd, targetDir);
  // Keep `process.env.PWD` in sync – many libraries rely on it.
  process.env.PWD = currentCwd;
};

// Override `process.cwd` so it returns our emulated value.
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
process.cwd = function mockedCwd() {
  return currentCwd;
};
