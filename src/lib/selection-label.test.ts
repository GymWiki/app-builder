import { describe, expect, it } from "vitest";
import { kindOf, rangesOf, summarize } from "./selection-label";
import type { Selection } from "./types";

const t = (widget: string, text: string | null, start = 10, end = 12) => ({
  widget,
  text,
  file: "lib/settings_screen.dart",
  startLine: start,
  endLine: end,
  chain: [],
  viaShared: false,
});

describe("selection summary", () => {
  it("classifies widgets in words a user knows", () => {
    expect(
      ["PrimaryButton", "ElevatedButton", "Text", "ListTile", "Icon", "Image", "TextField", "AppBar", "Column"].map(
        kindOf,
      ),
    ).toEqual(["button", "button", "text", "listItem", "icon", "image", "input", "bar", "other"]);
  });

  it("names one element with its text and screen, trimming long text", () => {
    const sel: Selection = {
      screen: "SettingsScreen",
      screenFile: "lib/settings_screen.dart",
      targets: [t("PrimaryButton", "Save")],
      bbox: null,
    };
    expect(summarize(sel)).toMatchObject({
      kind: "single",
      element: "button",
      text: "Save",
      screen: "Settings",
      count: 1,
    });
    const long = summarize({ ...sel, targets: [t("Text", "x".repeat(60))] });
    expect(long.text?.length).toBe(28);
  });

  it("counts several elements and hands over the allowed ranges", () => {
    const sel: Selection = {
      screen: null,
      screenFile: null,
      targets: [t("Text", "A", 5, 5), t("Text", "B", 7, 9)],
      bbox: null,
    };
    expect(summarize(sel)).toMatchObject({ kind: "many", count: 2, screen: null });
    expect(rangesOf(sel)).toEqual([
      ["lib/settings_screen.dart", 5, 5],
      ["lib/settings_screen.dart", 7, 9],
    ]);
  });
});
