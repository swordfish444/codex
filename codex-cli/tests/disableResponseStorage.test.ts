/**
 * codex/codex-cli/tests/disableResponseStorage.test.ts
 */

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { loadConfig, saveConfig } from "../src/utils/config";

const sandboxHome = mkdtempSync(join(tmpdir(), "codex-home-"));
const codexDir = join(sandboxHome, ".codex");
const yamlPath = join(codexDir, "config.yaml");

describe("disableResponseStorage persistence", () => {
  beforeAll(() => {
    // mkdir -p ~/.codex inside the sandbox
    rmSync(codexDir, { recursive: true, force: true });
    mkdirSync(codexDir, { recursive: true });

    // seed YAML with ZDR enabled
    writeFileSync(yamlPath, "model: o4-mini\ndisableResponseStorage: true\n");
  });

  afterAll(() => {
    rmSync(sandboxHome, { recursive: true, force: true });
  });

  it("keeps disableResponseStorage=true across load/save cycle", async () => {
    // 1️⃣ explicitly load the sandbox file
    const cfg1 = loadConfig(yamlPath);
    expect(cfg1.disableResponseStorage).toBe(true);

    // 2️⃣ save right back to the same file
    await saveConfig(cfg1, yamlPath);

    // 3️⃣ reload and re-assert
    const cfg2 = loadConfig(yamlPath);
    expect(cfg2.disableResponseStorage).toBe(true);
  });
});
