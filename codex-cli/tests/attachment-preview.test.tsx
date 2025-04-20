// Attachment preview shows selected images and clears with Ctrl+U

import fs from "node:fs";
import path from "node:path";
import React from "react";
import { beforeAll, afterAll, describe, expect, it, vi } from "vitest";

import { renderTui } from "./ui-test-helpers.js";

vi.mock("../src/utils/input-utils.js", () => ({
  createInputItem: vi.fn(async () => ({})),
  imageFilenameByDataUrl: new Map(),
}));

// mock external deps used inside chat input
vi.mock("../../approvals.js", () => ({ isSafeCommand: () => null }));
vi.mock("../src/format-command.js", () => ({
  // Accept an array of command tokens and join them with spaces for display.
  formatCommandForDisplay: (c: Array<string>): string => c.join(" "),
}));

import TerminalChatInput from "../src/components/chat/terminal-chat-input.js";

async function type(
  stdin: NodeJS.WritableStream & { write(str: string): void },
  text: string,
  flush: () => Promise<void>,
): Promise<void> {
  stdin.write(text);
  await flush();
}

function props() {
  return {
    isNew: true,
    loading: false,
    submitInput: () => {},
    confirmationPrompt: null,
    submitConfirmation: () => {},
    setLastResponseId: () => {},
    setItems: () => {},
    contextLeftPercent: 100,
    openOverlay: () => {},
    openModelOverlay: () => {},
    openApprovalOverlay: () => {},
    openHelpOverlay: () => {},
    interruptAgent: () => {},
    active: true,
    onCompact: () => {},
  };
}

describe("Chat input attachment preview", () => {
  const TMP = path.join(process.cwd(), "attachment-preview-test");
  const IMG = path.join(TMP, "foo.png");

  beforeAll(() => {
    fs.mkdirSync(TMP, { recursive: true });
    fs.writeFileSync(IMG, "");
  });

  afterAll(() => {
    fs.rmSync(TMP, { recursive: true, force: true });
  });

  it("shows image then clears with Ctrl+U", async () => {
    const orig = process.cwd();
    process.chdir(TMP);

    const { stdin, flush, lastFrameStripped, cleanup } = renderTui(
      React.createElement(TerminalChatInput, props())
    );

    await flush();

    await type(stdin, "@", flush);
    await type(stdin, "\r", flush); // choose first

    const frame1 = lastFrameStripped();
    expect(frame1.match(/foo\.png/g)?.length ?? 0).toBe(1);

    await type(stdin, "\x15", flush); // Ctrl+U

    expect(lastFrameStripped()).not.toContain("foo.png");

    cleanup();
    process.chdir(orig);
  });
});
