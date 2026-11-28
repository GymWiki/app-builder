import { describe, expect, it } from "vitest";
import { classify, fanoutPlan, folderPages, scopePages, toggleMulti } from "./scope";
import type { FolderState } from "./screens-folders";

const all = ["Home", "Settings", "About", "Profile", "Item", "Cart"];

describe("scope selection", () => {
  it("the whole app is every page; a folder is its pages including nested folders", () => {
    expect(scopePages({ kind: "app" }, all)).toEqual(all);
    const s: FolderState = {
      folders: [
        { id: "f1", name: "Settings", parent: null },
        { id: "f2", name: "Deep", parent: "f1" },
      ],
      placed: { Settings: "f1", About: "f2", Profile: "f1" },
      closed: [],
    };
    expect(folderPages(s, "f1", all)).toEqual(["Settings", "About", "Profile"]);
    expect(folderPages(s, "f2", all)).toEqual(["About"]);
  });

  it("ctrl-click toggles and shift-click selects a range in list order", () => {
    const o = { shift: false, anchor: null, order: all };
    expect(toggleMulti(["Home"], "About", o)).toEqual(["Home", "About"]);
    expect(toggleMulti(["Home", "About"], "Home", o)).toEqual(["About"]);
    expect(toggleMulti(["Home"], "Profile", { shift: true, anchor: "Home", order: all })).toEqual(["Home", "Settings", "About", "Profile"]);
  });
});

describe("classify: breadth and difficulty are separate signals", () => {
  it("a wide but simple change fans out over the pages", () => {
    expect(classify("Make all buttons rounder", 7)).toEqual({ breadth: "wide", difficulty: "simple", strategy: "fanout" });
    expect(classify("Gebruik overal hetzelfde lettertype", 4).strategy).toBe("fanout");
  });
  it("a wide and complex change is planned first, in one run", () => {
    expect(classify("Change the navigation to a bottom bar", 6)).toEqual({ breadth: "wide", difficulty: "complex", strategy: "plan" });
    expect(classify("Hernoem het gegevensmodel Speler naar Deelnemer", 5).strategy).toBe("plan");
  });
  it("one or two pages is a normal run, however hard", () => {
    expect(classify("Rewrite the data model", 2).strategy).toBe("single");
    expect(classify("Make it blue", 1).strategy).toBe("single");
  });
  it("the fan-out plan is shared first, then one small task per page", () => {
    expect(fanoutPlan(["A", "B"])).toEqual([{ kind: "shared" }, { kind: "page", page: "A" }, { kind: "page", page: "B" }]);
  });
});
