import { renderTui } from "./ui-test-helpers.js";
import { Markdown } from "../src/components/chat/terminal-chat-response-item.js";
import React from "react";
import { it, expect } from "vitest";

/** Simple sanity check that the Markdown component renders bold/italic text.
 * We strip ANSI codes, so the output should contain the raw words. */
it("renders basic markdown", () => {
  const { lastFrameStripped } = renderTui(
    <Markdown fileOpener={undefined}>**bold** _italic_</Markdown>,
  );

  const frame = lastFrameStripped();
  expect(frame).toContain("bold");
  expect(frame).toContain("italic");
});

it("renders markdown with citations", () => {
  const { lastFrame } = renderTui(
    <Markdown fileOpener={"vscode"} cwd="/foo/bar">
      File with TODO: 【F:src/approvals.ts†L40】
    </Markdown>,
  );

  const outputWithAnsi = lastFrame();
  expect(outputWithAnsi).toBe(
    "File with TODO:" +
      "\x1B[0m\x1B[34m\x1B]8;;vscode://file/foo/bar/src/approvals.ts:40\x07\x1B[34m\x1B[4msrc/approvals.ts\x1B[24m\x1B[39m\x1B[34m\x1B]8;;\x07\x1B[39m\x1B[0m\n" +
      "\n",
  );
});
