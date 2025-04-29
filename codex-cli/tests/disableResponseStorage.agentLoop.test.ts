/**
 * codex-cli/tests/disableResponseStorage.agentLoop.test.ts
 *
 * Verifies AgentLoop's request-building logic for both values of
 * disableResponseStorage.
 */

import { describe, it, expect, vi } from "vitest";
import { AgentLoop } from "../src/utils/agent/agent-loop";
import type { AppConfig } from "../src/utils/config";
// If you have a ReviewDecision type or enum, import it here:
// import type { ReviewDecision } from "../src/utils/agent/types";

/* ─────────── 1.  Spy + module mock ──────────────────────────────── */
const createSpy = vi.fn().mockResolvedValue({
  data: { id: "resp_123", status: "completed", output: [] },
});

vi.mock("openai", () => ({
  default: class {
    public responses = { create: createSpy };
  },
  APIConnectionTimeoutError: class extends Error {},
}));

/* ─────────── 2.  Parametrised tests ─────────────────────────────── */
describe.each([
  { flag: true, title: "omits previous_response_id & sets store:false" },
  { flag: false, title: "sends previous_response_id & allows store:true" },
])("AgentLoop with disableResponseStorage=%s", ({ flag, title }) => {
  /* build a fresh config for each case */
  const cfg: AppConfig = {
    model: "o4-mini",
    provider: "openai",
    instructions: "",
    disableResponseStorage: flag,
    notify: false,
  };

  it(title, async () => {
    /* reset spy per iteration */
    createSpy.mockClear();

    const loop = new AgentLoop({
      model: cfg.model,
      provider: cfg.provider,
      config: cfg,
      instructions: "",
      approvalPolicy: "suggest",
      disableResponseStorage: flag,
      additionalWritableRoots: [],
      onItem() {},
      onLoading() {},
      getCommandConfirmation: async () => ({ review: "approved" }),
      onLastResponseId() {},
    });

    await loop.run([
      {
        type: "message",
        content: [{ type: "text", text: "hello" }],
      },
    ]);

    expect(createSpy).toHaveBeenCalledTimes(1);

    const payload = createSpy.mock.calls[0][0];

    if (flag) {
      /* behaviour when ZDR is *on* */
      expect(payload).not.toHaveProperty("previous_response_id");
      if (payload.input) {
        payload.input.forEach((m: any) =>
          expect(m.store === undefined ? false : m.store).toBe(false),
        );
      }
    } else {
      /* behaviour when ZDR is *off* */
      expect(payload).toHaveProperty("previous_response_id");
      if (payload.input) {
        // first user message is usually non-stored; assistant messages will be stored
        // so here we just assert the property is not forcibly set to false
        payload.input.forEach((m: any) => {
          if ("store" in m) {
            expect(m.store).not.toBe(false);
          }
        });
      }
    }
  });
});
