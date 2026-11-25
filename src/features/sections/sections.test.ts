import { describe, expect, it } from "vitest";
import type { Questionnaire, ScreenGraph } from "@/lib/types";
import { treeRows } from "@/features/project/screens-list";
import { summaryKeys } from "@/features/sections/publish/listing-tab";
import { isHex } from "@/features/sections/theme-panel";

const node = (c: string, children: string[] = []) => ({ class: c, file: `lib/${c}.dart`, line: 1, children });

describe("screen tree", () => {
  it("indents children under their parent and lists each screen once", () => {
    const g: ScreenGraph = {
      home: "Home",
      nodes: [node("Home", ["Settings", "About"]), node("Settings", ["About"]), node("About"), node("Lonely")],
      unlinked: ["Lonely"],
    };
    const { rows, unlinked } = treeRows(g);
    expect(rows.map((r) => [r.class, r.depth])).toEqual([
      ["Home", 0],
      ["Settings", 1],
      ["About", 2],
    ]);
    expect(unlinked.map((r) => r.class)).toEqual(["Lonely"]);
  });

  it("survives cycles (A → B → A)", () => {
    const g: ScreenGraph = { home: "A", nodes: [node("A", ["B"]), node("B", ["A"])], unlinked: [] };
    expect(treeRows(g).rows.map((r) => r.class)).toEqual(["A", "B"]);
  });

  it("handles a project without screens", () => {
    expect(treeRows({ home: null, nodes: [], unlinked: [] })).toEqual({ rows: [], unlinked: [] });
  });
});

describe("theme color", () => {
  it("accepts only #RRGGBB", () => {
    expect(isHex("#4F46E5")).toBe(true);
    expect(isHex("#4f46e5")).toBe(true);
    for (const bad of ["4F46E5", "#4F46E", "#GGGGGG", "red", "#4F46E5 "]) expect(isHex(bad)).toBe(false);
  });
});

describe("store questionnaire summary", () => {
  const q = (p: Partial<Questionnaire>): Questionnaire => ({
    hasAccounts: false,
    collectsPersonalData: false,
    showsAds: false,
    inAppPurchases: false,
    userGeneratedContent: false,
    usesLocation: false,
    playConsoleDone: false,
    ...p,
  });
  it("says nothing special is needed when every answer is no", () => {
    expect(summaryKeys(q({}))).toEqual(["none"]);
  });
  it("lists a declaration per yes", () => {
    expect(summaryKeys(q({ collectsPersonalData: true, showsAds: true, hasAccounts: true }))).toEqual([
      "privacy",
      "ads",
      "accounts",
    ]);
  });
});
