import { describe, expect, it } from "vitest";
import { timeAgo } from "./format";

const ago = (ms: number) => new Date(Date.now() - ms).toISOString();

describe("timeAgo", () => {
  it("formats minutes, hours and days per locale", () => {
    expect(timeAgo(ago(12 * 60_000), "en")).toBe("12 minutes ago");
    expect(timeAgo(ago(3 * 3600_000), "nl")).toBe("3 uur geleden");
    expect(timeAgo(ago(26 * 3600_000), "en")).toBe("yesterday");
  });
  it("returns empty text for invalid dates", () => {
    expect(timeAgo("nope", "en")).toBe("");
  });
});
