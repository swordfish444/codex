import fs from "node:fs";
import path from "node:path";
import React from "react";
import { beforeAll, afterAll, describe, expect, it, vi } from "vitest";

import { renderTui } from "./ui-test-helpers.js";

vi.mock("../src/utils/input-utils.js", () => ({
  createInputItem: vi.fn(async () => ({})),
  imageFilenameByDataUrl: new Map(),
}));

import ImagePickerOverlay from "../src/components/chat/image-picker-overlay.js";

async function type(
  stdin: NodeJS.WritableStream & { write(str: string): void },
  text: string,
  flush: () => Promise<void>,
): Promise<void> {
  stdin.write(text);
  await flush();
}

describe("Image picker overlay", () => {
  let TMP: string;
  let CHILD: string;

  beforeAll(() => {
    TMP = fs.mkdtempSync(path.join(process.cwd(), "overlay-test-"));
    CHILD = path.join(TMP, "child");
    fs.mkdirSync(CHILD, { recursive: true });
    fs.writeFileSync(path.join(TMP, "a.png"), "");
    fs.writeFileSync(path.join(TMP, "b.png"), "");
    fs.writeFileSync(path.join(CHILD, "nested.png"), "");
  });

  afterAll(() => {
    fs.rmSync(TMP, { recursive: true, force: true });
  });

  it("shows ../ when below root and selects it", async () => {
    const onChangeDir = vi.fn();
    const { lastFrameStripped, stdin, flush } = renderTui(
      React.createElement(ImagePickerOverlay, {
        rootDir: TMP,
        cwd: CHILD,
        onPick: () => {},
        onCancel: () => {},
        onChangeDir,
      })
    );

    await flush();
    expect(lastFrameStripped()).toContain("❯ ../");
    await type(stdin, "\r", flush);
    expect(onChangeDir).toHaveBeenCalledWith(path.dirname(CHILD));
  });

  it("selecting file calls onPick", async () => {
    const onPick = vi.fn();
    const { stdin, flush } = renderTui(
      React.createElement(ImagePickerOverlay, {
        rootDir: TMP,
        cwd: TMP,
        onPick,
        onCancel: () => {},
        onChangeDir: () => {},
      })
    );
    await flush();
    await type(stdin, "\r", flush);
    expect(onPick).toHaveBeenCalledWith(path.join(TMP, "a.png"));
  });
});
