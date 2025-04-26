import { describe, expect, test } from "vitest";

import {
  estimateCostUSD,
  estimateCostFromUsage,
} from "../src/utils/estimate-cost.js";
import { SessionCostTracker } from "../src/utils/session-cost.js";
import type { ResponseItem } from "openai/resources/responses/responses.mjs";

// Helper to craft a minimal ResponseItem for tests
function makeMessage(
  id: string,
  role: "user" | "assistant",
  text: string,
): ResponseItem {
  return {
    id,
    type: "message",
    role,
    content: [{ type: role === "user" ? "input_text" : "output_text", text }],
  } as ResponseItem;
}

describe("estimateCostUSD", () => {
  test("returns a proportional, positive estimate for known models", () => {
    const items: Array<ResponseItem> = [
      makeMessage("1", "user", "hello world"),
      makeMessage("2", "assistant", "hi there"),
    ];

    const cost = estimateCostUSD(items, "gpt-3.5-turbo");
    expect(cost).not.toBeNull();
    expect(cost!).toBeGreaterThan(0);

    // Adding another token should increase the estimate
    const cost2 = estimateCostUSD(
      items.concat([makeMessage("3", "user", "extra")]),
      "gpt-3.5-turbo",
    );
    expect(cost2!).toBeGreaterThan(cost!);
  });

  test("cost calculation honours cached input token discount", () => {
    const usage = {
      input_tokens: 1000,
      input_tokens_details: { cached_tokens: 600 },
      output_tokens: 500,
      total_tokens: 1500,
    } as any; // simple literal structure for test

    const cost = estimateCostFromUsage(usage, "gpt-4.1");

    // Expected: (1000-600)*0.000002 + 600*0.0000005 + 500*0.000008
    const expected = 400 * 0.000002 + 600 * 0.0000005 + 500 * 0.000008;
    expect(cost).not.toBeNull();
    expect(cost!).toBeCloseTo(expected, 8);
  });
});

describe("SessionCostTracker", () => {
  test("accumulates items and reports tokens & cost", () => {
    const tracker = new SessionCostTracker("gpt-3.5-turbo");
    tracker.addItems([makeMessage("1", "user", "foo")]);
    tracker.addItems([makeMessage("2", "assistant", "bar baz")]);

    expect(tracker.getTokensUsed()).toBeGreaterThan(0);
    expect(tracker.getCostUSD()!).toBeGreaterThan(0);
  });
});
