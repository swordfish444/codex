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

// We need to capture a reference to the mocked `createInputItem` function so we
// can make assertions later in the test, _and_ respect Vitest’s requirement
// that any variables used inside the `vi.mock` factory are already defined at
// the time the factory is hoisted.  To satisfy both constraints we:
//   1. Declare the variable with `let` (so it’s hoisted), **without** assigning
//      a value yet.
//   2. Inside the factory, create the mock with `vi.fn()` and assign it to the
//      outer-scoped variable before returning it.
// This avoids the “there was an error when mocking a module” failure that
// occurs when a factory closes over an uninitialised top-level `const`.

// Using `var` ensures the binding is hoisted, so it exists (as `undefined`) at
// the time the `vi.mock` factory runs. We re-assign it inside the factory.
// eslint-disable-next-line no-var
var createInputItemMock!: ReturnType<typeof vi.fn>;

vi.mock("../src/utils/input-utils.js", () => {
  // Initialise the mock lazily inside the factory so the reference is valid
  // when the module is evaluated.
  createInputItemMock = vi.fn(async () => ({}));

  return {
    createInputItem: createInputItemMock,
    imageFilenameByDataUrl: new Map(),
  };
});
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
    openDiffOverlay: () => {},
    thinkingSeconds: 0,
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
    process.env["DEBUG_TCI"] = "1";
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

    const frame = lastFrameStripped();

    expect(frame.match(/dropped\.png/g)?.length ?? 0).toBe(1);

    // Now submit the message.
    await type(stdin, "\r", flush);
    await flush();

    // createInputItem should have been called with the dropped image path
    expect(createInputItemMock).toHaveBeenCalled();
    const calls: Array<Array<unknown>> = createInputItemMock.mock.calls as any;
    const lastCall = calls[calls.length - 1] as Array<unknown>;
    expect(lastCall?.[1 as number]).toEqual(["dropped.png"]);

    cleanup();
    process.chdir(orig);
  });

  it("does NOT show slash-command overlay for absolute paths", async () => {
    const orig = process.cwd();
    process.chdir(TMP);

    const { stdin, flush, lastFrameStripped, cleanup } = renderTui(
      React.createElement(TerminalChatInput, props()),
    );

    await flush();

    // absolute path starting with '/'
    const absPath = path.join(TMP, "dropped.png");
    await type(stdin, `${absPath} `, flush);
    await flush();

    const frame = lastFrameStripped();

    // Should contain attachment preview but NOT typical slash-command suggestion like "/help"
    expect(frame).toContain("dropped.png");
    expect(frame).not.toContain("/help");

    cleanup();
    process.chdir(orig);
  });
});
