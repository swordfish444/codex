import { describe, expect, it } from "vitest";

import { extractImagePaths } from "../src/utils/image-detector.js";

describe("extractImagePaths", () => {
  it("detects markdown image", () => {
    const { paths, text } = extractImagePaths(
      "hello ![alt](foo/bar.png) world",
    );
    expect(paths).toEqual(["foo/bar.png"]);
    expect(text).toBe("hello  world");
  });

  it("detects quoted image", () => {
    const { paths, text } = extractImagePaths('drag "baz.jpg" here');
    expect(paths).toEqual(["baz.jpg"]);
    expect(text).toBe("drag  here");
  });

  it("detects bare path", () => {
    const { paths, text } = extractImagePaths("see /tmp/img.gif please");
    expect(paths).toEqual(["/tmp/img.gif"]);
    expect(text).toBe("see  please");
  });
});
