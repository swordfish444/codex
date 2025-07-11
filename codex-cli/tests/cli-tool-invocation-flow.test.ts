import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import fs from "fs";
import os from "os";
import path from "path";

const originalArgv = process.argv.slice();
const originalCwd = process.cwd();
const originalEnv = { ...process.env };
const originalNode = process.versions.node;
const originalPlatform = process.platform;

// Utility to setup mocks common to CLI tests
function setupCommonMocks(tmpDir: string) {
  vi.doMock("../src/utils/logger/log.js", () => ({
    __esModule: true,
    initLogger: () => ({ isLoggingEnabled: () => false, log: () => {} }),
  }));
  vi.doMock("../src/utils/check-updates.js", () => ({
    __esModule: true,
    checkForUpdates: vi.fn(),
  }));
  vi.doMock("../src/utils/get-api-key.js", () => ({
    __esModule: true,
    getApiKey: () => "test-key",
    maybeRedeemCredits: async () => {},
  }));
  vi.doMock("../src/approvals.js", () => ({
    __esModule: true,
    alwaysApprovedCommands: new Set<string>(),
    canAutoApprove: () => ({ type: "auto-approve", runInSandbox: false }),
    isSafeCommand: () => null,
  }));
  vi.doMock("src/approvals.js", () => ({
    __esModule: true,
    alwaysApprovedCommands: new Set<string>(),
    canAutoApprove: () => ({ type: "auto-approve", runInSandbox: false }),
    isSafeCommand: () => null,
  }));
  vi.doMock("../src/format-command.js", () => ({
    __esModule: true,
    formatCommandForDisplay: (cmd: Array<string>) => cmd.join(" "),
  }));
  vi.doMock("src/format-command.js", () => ({
    __esModule: true,
    formatCommandForDisplay: (cmd: Array<string>) => cmd.join(" "),
  }));
  vi.doMock("../src/utils/agent/log.js", () => ({
    __esModule: true,
    log: () => {},
    isLoggingEnabled: () => false,
  }));
  vi.doMock("../src/utils/config.js", () => ({
    __esModule: true,
    loadConfig: () => ({
      model: "test-model",
      instructions: "",
      provider: "openai",
      notify: false,
      approvalMode: undefined,
      tools: { shell: { maxBytes: 1024, maxLines: 100 } },
      disableResponseStorage: false,
      reasoningEffort: "medium",
    }),
    PRETTY_PRINT: true,
    INSTRUCTIONS_FILEPATH: path.join(tmpDir, "instructions.md"),
  }));
}

describe("CLI Tool Invocation Flow", () => {
  let tmpDir: string;

  beforeEach(() => {
    vi.resetModules();
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "codex-cli-test-"));
    process.chdir(tmpDir);
    process.env.CODEX_UNSAFE_ALLOW_NO_SANDBOX = "1";
    Object.defineProperty(process.versions, "node", { value: "22.0.0" });
    Object.defineProperty(process, "platform", { value: "win32" });
    setupCommonMocks(tmpDir);
  });

  afterEach(() => {
    process.argv = originalArgv.slice();
    process.chdir(originalCwd);
    process.env = { ...originalEnv };
    Object.defineProperty(process.versions, "node", { value: originalNode });
    Object.defineProperty(process, "platform", { value: originalPlatform });
    vi.restoreAllMocks();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it("executes a shell command returned by the model", async () => {
    class CallStream {
      async *[Symbol.asyncIterator]() {
        yield {
          type: "response.output_item.done",
          item: {
            type: "function_call",
            id: "call_1",
            name: "shell",
            arguments: JSON.stringify({ cmd: ["echo", "Hello"] }),
          },
        } as any;
        yield {
          type: "response.completed",
          response: {
            id: "resp1",
            status: "completed",
            output: [
              {
                type: "function_call",
                id: "call_1",
                name: "shell",
                arguments: JSON.stringify({ cmd: ["echo", "Hello"] }),
              },
            ],
          },
        } as any;
      }
    }

    class DoneStream {
      async *[Symbol.asyncIterator]() {
        yield {
          type: "response.output_item.done",
          item: {
            type: "message",
            role: "assistant",
            content: [{ type: "output_text", text: "done" }],
          },
        } as any;
        yield {
          type: "response.completed",
          response: {
            id: "resp2",
            status: "completed",
            output: [
              {
                type: "message",
                role: "assistant",
                content: [{ type: "output_text", text: "done" }],
              },
            ],
          },
        } as any;
      }
    }

    let call = 0;
    vi.mock("openai", () => {
      return {
        __esModule: true,
        default: class FakeOpenAI {
          public responses = {
            create: async () => {
              call += 1;
              return call === 1 ? new CallStream() : new DoneStream();
            },
          };
        },
        APIConnectionTimeoutError: class APIConnectionTimeoutError extends Error {},
      };
    });

    const logs: Array<string> = [];
    vi.spyOn(console, "log").mockImplementation((...args) => {
      logs.push(args.join(" "));
    });

    vi.spyOn(process, "exit").mockImplementation(((code?: number) => {
      throw new Error(`exit:${code}`);
    }) as any);

    process.argv = ["node", "codex", "--full-auto", "-q", "hi"];

    await expect(import("../src/cli.tsx")).rejects.toThrow("exit:0");

    const hasOutput = logs.some((l) => l.includes("command.stdout") && l.includes("Hello"));
    expect(hasOutput).toBe(true);
  });
});
