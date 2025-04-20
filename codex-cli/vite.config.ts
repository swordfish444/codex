import { defineConfig } from "vite";

/**
 * Vite configuration used by the Codex CLI package.  The build process itself
 * doesn’t rely on Vite’s bundling features – we only ship this file so that
 * Vitest can pick it up when executing the unit‑test suite.  The only custom
 * logic we currently inject is a *test* configuration block that registers a
 * small setup script executed in each worker thread before any test files are
 * loaded.  That script polyfills `process.chdir()` which is disallowed inside
 * Node.js workers as of v22 and would otherwise throw when some tests attempt
 * to change the working directory.
 */

export default defineConfig({
  test: {
    // Execute tests inside worker threads but force Vitest to spawn *only one*
    // worker.  This keeps the environment isolation that some components
    // depend on while avoiding a `tinypool` recursion bug that occasionally
    // triggers when multiple workers are used.
    pool: "threads",
    poolOptions: {
      threads: {
        minThreads: 1,
        maxThreads: 1,
      },
    },
    /**
     * Register the setup file.  We use a relative path so that Vitest resolves
     * it against the project root irrespective of where the CLI is executed.
     */
    setupFiles: ["./tests/test-setup.js"],
  },
});
