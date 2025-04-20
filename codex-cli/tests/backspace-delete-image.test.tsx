// Backspace removes last attached image when draft is empty

import fs from "node:fs";
import path from "node:path";
import React from "react";
import { beforeAll, afterAll, describe, expect, it, vi } from "vitest";

import { renderTui } from "./ui-test-helpers.js";

vi.mock("../src/utils/input-utils.js", () => ({
  createInputItem: vi.fn(async () => ({})),
  imageFilenameByDataUrl: new Map(),
}));
vi.mock("../../approvals.js", () => ({ isSafeCommand: () => null }));
vi.mock("../src/format-command.js", () => ({
  formatCommandForDisplay: (c) => c.join(" "),
}));

import TerminalChatInput from "../src/components/chat/terminal-chat-input.js";

async function type(stdin, text, flush) {
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

describe("Backspace deletes attached image", () => {
  const TMP = path.join(process.cwd(), "backspace-delete-image-test");
  const IMG = path.join(TMP, "bar.png");

  beforeAll(() => {
    fs.mkdirSync(TMP, { recursive: true });
    fs.writeFileSync(IMG, "");
  });

  afterAll(() => {
    fs.rmSync(TMP, { recursive: true, force: true });
  });

  it("removes image on backspace", async () => {
    const orig = process.cwd();
    process.chdir(TMP);

    const { stdin, flush, lastFrameStripped, cleanup } = renderTui(
      React.createElement(TerminalChatInput, props())
    );

    await flush();

    await type(stdin, "@", flush);
    console.log('AFTER @', lastFrameStripped());
    await type(stdin, "\r", flush);
    console.log('FRAME1', lastFrameStripped());
    expect(lastFrameStripped()).toContain("bar.png");

    await type(stdin, "\x7f", flush);

    expect(lastFrameStripped()).not.toContain("bar.png");

    cleanup();
    process.chdir(orig);
  });
});
