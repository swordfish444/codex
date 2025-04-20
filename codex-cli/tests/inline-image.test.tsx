import React from "react";
import { describe, expect, it } from "vitest";

import { renderTui } from "./ui-test-helpers.js";

import TerminalInlineImage from "../src/components/chat/terminal-inline-image.js";
import TerminalChatResponseItem from "../src/components/chat/terminal-chat-response-item.js";
import {
  imageFilenameByDataUrl,
  createInputItem,
} from "../src/utils/input-utils.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

import path from "node:path";
import fs from "node:fs";

describe("TerminalInlineImage fallback", () => {
  it("renders alt text in test env", () => {
    const { lastFrameStripped } = renderTui(
      <TerminalInlineImage src={Buffer.from("abc")} alt="placeholder" />,
    );
    expect(lastFrameStripped()).toContain("placeholder");
  });
});

function fakeImageMessage(filename: string) {
  const url = "data:image/png;base64,AAA";
  imageFilenameByDataUrl.set(url, filename);
  return {
    id: "test-id",
    type: "message",
    role: "user",
    content: [
      { type: "input_text", text: "hello" },
      { type: "input_image", detail: "auto", image_url: url },
    ],
  };
}

describe("TerminalChatResponseItem image label", () => {
  it("shows filename", () => {
    const msg = fakeImageMessage("sample.png");
    const { lastFrameStripped } = renderTui(
      <TerminalChatResponseItem item={msg as any} />,
    );
    expect(lastFrameStripped()).toContain('<Image path="sample.png">');
  });
});

// ---------------------------------------------------------------------------
// New tests – ensure createInputItem gracefully skips missing images.
// ---------------------------------------------------------------------------

describe("createInputItem – missing images", () => {
  it("ignores images that never existed on disk (conversation start)", async () => {
    const item = await createInputItem("hello", ["ghost.png"]);
    expect(item.content.some((c) => c.type === "input_image")).toBe(false);
  });

  it("ignores images deleted before submit (mid‑conversation)", async () => {
    const tmpDir = fs.mkdtempSync(path.join(process.cwd(), "missing-img-"));
    const imgPath = path.join(tmpDir, "temp.png");
    fs.writeFileSync(imgPath, "dummy");

    // Remove the file before we construct the message.
    fs.rmSync(imgPath);

    const item = await createInputItem("", [imgPath]);
    expect(item.content.some((c) => c.type === "input_image")).toBe(false);

    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  // Additional integration tests for the system‑level warning are covered in
  // higher‑level suites. This unit file focuses on createInputItem behaviour.
});
