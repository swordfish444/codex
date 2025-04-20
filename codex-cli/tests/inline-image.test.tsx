import React from "react";
import { describe, expect, it } from "vitest";

import { renderTui } from "./ui-test-helpers.js";

import TerminalInlineImage from "../src/components/chat/terminal-inline-image.js";
import TerminalChatResponseItem from "../src/components/chat/terminal-chat-response-item.js";
import { imageFilenameByDataUrl } from "../src/utils/input-utils.js";

describe("TerminalInlineImage fallback", () => {
  it("renders alt text in test env", () => {
    const { lastFrameStripped } = renderTui(
      <TerminalInlineImage src={Buffer.from("abc")} alt="placeholder" />
    );
    expect(lastFrameStripped()).toContain("placeholder");
  });
});

function fakeImageMessage(filename) {
  const url = "data:image/png;base64,AAA";
  imageFilenameByDataUrl.set(url, filename);
  return {
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
      <TerminalChatResponseItem item={msg} />
    );
    expect(lastFrameStripped()).toContain("sample.png");
  });
});
