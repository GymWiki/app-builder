import { describe, expect, it } from "vitest";
import {
  EMPTY,
  addFolder,
  deleteFolder,
  flatten,
  isInside,
  moveFolder,
  moveScreen,
  prune,
  renameFolder,
  toggleFolder,
} from "./screens-folders";

const screens = ["Home", "Settings", "About"];

describe("screens-folders", () => {
  it("ignores empty names and renames", () => {
    expect(addFolder(EMPTY, "  ", null)).toBe(EMPTY);
    const s = addFolder(EMPTY, " Account ", null);
    expect(s.folders[0]!.name).toBe("Account");
    expect(renameFolder(s, s.folders[0]!.id, "Profile").folders[0]!.name).toBe("Profile");
  });

  it("puts screens in folders and lists them under their folder", () => {
    let s = addFolder(EMPTY, "Account", null);
    const id = s.folders[0]!.id;
    s = moveScreen(s, "Settings", id);
    const flat = flatten(s, screens);
    expect(flat.map((i) => (i.kind === "folder" ? `F:${i.folder.name}` : i.class))).toEqual([
      "F:Account",
      "Settings",
      "Home",
      "About",
    ]);
    expect(flat[1]!.depth).toBe(1);
    s = toggleFolder(s, id);
    expect(flatten(s, screens).map((i) => i.kind)).toEqual(["folder", "screen", "screen"]);
  });

  it("deleting a folder moves its content up and never loses a screen", () => {
    let s = addFolder(EMPTY, "A", null);
    const a = s.folders[0]!.id;
    s = addFolder(s, "B", a);
    const b = s.folders[1]!.id;
    s = moveScreen(moveScreen(s, "About", b), "Home", a);
    s = deleteFolder(s, a);
    expect(s.folders).toHaveLength(1);
    expect(s.folders[0]!.parent).toBeNull();
    expect(s.placed).toEqual({ About: b });
    s = deleteFolder(s, b);
    expect(flatten(s, screens).map((i) => i.kind)).toEqual(["screen", "screen", "screen"]);
  });

  it("refuses to move a folder into itself or its own child", () => {
    let s = addFolder(EMPTY, "A", null);
    s = addFolder(s, "B", s.folders[0]!.id);
    const [a, b] = s.folders.map((f) => f.id) as [string, string];
    expect(isInside(s, b, a)).toBe(true);
    expect(moveFolder(s, a, b)).toBe(s);
    expect(moveFolder(s, a, a)).toBe(s);
    expect(moveFolder(s, b, null).folders[1]!.parent).toBeNull();
  });

  it("prunes screens that no longer exist", () => {
    let s = addFolder(EMPTY, "A", null);
    s = moveScreen(s, "Gone", s.folders[0]!.id);
    expect(prune(s, screens).placed).toEqual({});
  });
});
