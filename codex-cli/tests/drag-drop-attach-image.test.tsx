// Dropping / pasting an image path into the chat input should immediately move
// that image into the attached-images preview and remove the path from the draft
// text.

import fs from "node:fs";
import path from "node:path";
import React from "react";
import { beforeAll, afterAll, describe, expect, it, vi } from "vitest";

import { renderTui } from "./ui-test-helpers.js";

// ---------------------------------------------------------------------------
// Mocks – keep in sync with other TerminalChatInput UI tests
// ---------------------------------------------------------------------------

const createInputItemMock = vi.fn(async (_text: string, _imgs: Array<string>) => ({}));

vi.mock("../src/utils/input-utils.js", () => ({
  createInputItem: createInputItemMock,
  imageFilenameByDataUrl: new Map(),
}));
vi.mock("../src/approvals.js", () => ({ isSafeCommand: () => null }));
vi.mock("../src/format-command.js", () => ({
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

describe("Drag-and-drop image attachment", () => {
  const TMP = path.join(process.cwd(), "drag-drop-image-test");
  const IMG = path.join(TMP, "dropped.png");

  beforeAll(() => {
    fs.mkdirSync(TMP, { recursive: true });
    fs.writeFileSync(IMG, "");
  });

  afterAll(() => {
    fs.rmSync(TMP, { recursive: true, force: true });
  });

  it("moves pasted path to attachment preview", async () => {
    process.env.DEBUG_TCI = "1";
    const orig = process.cwd();
    process.chdir(TMP);

    const { stdin, flush, lastFrameStripped, cleanup } = renderTui(
      React.createElement(TerminalChatInput, props()),
    );

    await flush(); // initial render

    // Simulate user pasting the bare filename (as most terminals do when you
    // drag a file).
    await type(stdin, "dropped.png ", flush);

    await flush();

    // A second flush to allow state updates triggered asynchronously by
    // setState inside the onChange handler.
    await flush();

    let frame = lastFrameStripped();


    expect(frame.match(/dropped\.png/g)?.length ?? 0).toBe(1);

    // Now submit the message.
    await type(stdin, "\r", flush);
    await flush();

    // createInputItem should have been called with the dropped image path
    expect(createInputItemMock).toHaveBeenCalled();
    const lastCall = createInputItemMock.mock.calls.at(-1);
    expect(lastCall?.[1]).toEqual(["dropped.png"]);

    cleanup();
    process.chdir(orig);
  });
});
