import React from "react";
import { render } from "ink-testing-library";
import stripAnsi from "strip-ansi";

export function renderTui(ui) {
  const { stdin, lastFrame, unmount, cleanup } = render(ui, {
    exitOnCtrlC: false,
  });

  // Some libraries assume these methods exist on TTY streams; add no‑ops.
  if (stdin && typeof stdin.ref !== "function") {
    // @ts-ignore
    stdin.ref = () => {};
  }
  if (stdin && typeof stdin.unref !== "function") {
    // @ts-ignore
    stdin.unref = () => {};
  }

  const lastFrameStripped = () => stripAnsi(lastFrame() ?? "");

  async function flush() {
    // wait one tick for Ink to process
    await new Promise((resolve) => setTimeout(resolve, 0));
  }

  return { stdin, lastFrame, lastFrameStripped, unmount, cleanup, flush };
}
