import { truncateMiddle, sessionScopedApprovalLabel } from "../src/utils/string-utils";

describe("truncateMiddle", () => {
  it("returns the original string when shorter than max length", () => {
    expect(truncateMiddle("short", 10)).toBe("short");
  });

  it("returns the original string when equal to max length", () => {
    expect(truncateMiddle("exactlen", 8)).toBe("exactlen");
  });

  it("truncates the middle of a longer string", () => {
    const text = "abcdefghij"; // length 10
    // maxLength 7 => trimLength=6, startLen=3, endLen=3 => "abc…hij"
    expect(truncateMiddle(text, 7)).toBe("abc…hij");
  });

  it("handles odd max lengths correctly", () => {
    const text = "abcdefghijkl"; // length 12
    // maxLength 8 => trimLength=7, startLen=4, endLen=3 => "abcd…ijk"
    expect(truncateMiddle(text, 8)).toBe("abcd…ijk");
  });
});

describe("sessionScopedApprovalLabel", () => {
  const cmd = "echo hello world";

  it("embeds the full command when shorter than max length", () => {
    expect(sessionScopedApprovalLabel(cmd, 50)).toBe(
      "Yes, always allow running `echo hello world` for this session (a)",
    );
  });

  it("embeds a truncated command when longer than max length", () => {
    const longCmd = "cat " + "a".repeat(100) + " end";
    const label = sessionScopedApprovalLabel(longCmd, 20);
    expect(label).toMatch(/^Yes, always allow running `.{0,20}` for this session \(a\)$/);
  });
});
