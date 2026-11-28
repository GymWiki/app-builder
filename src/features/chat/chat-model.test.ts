import { describe, expect, it } from "vitest";
import type { ChatEvent } from "@/lib/types";
import { applyEvent, replay, type Item } from "./chat-model";

const tool = (id: string, kind: "edit" | "command" = "edit"): ChatEvent => ({
  type: "tool_use",
  id,
  name: "Edit",
  kind,
  summary: "lib/main.dart",
  path: "lib/main.dart",
  detail: null,
});

describe("chat model", () => {
  it("streams text and replaces the live text with the final text (no duplicates)", () => {
    const items = replay([
      { type: "text_delta", message_id: "m1", text: "Hel" },
      { type: "text_delta", message_id: "m1", text: "lo" },
    ]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "assistant", text: "Hello", live: true });
    const done = applyEvent(items, { type: "text", message_id: "m1", text: "Hello!" });
    expect(done).toHaveLength(1);
    expect(done[0]).toMatchObject({ text: "Hello!", live: false });
  });

  it("final text without earlier deltas is added once", () => {
    const items = replay([{ type: "text", message_id: "m2", text: "Hi" }]);
    expect(items).toHaveLength(1);
  });

  it("tracks tool status from running to ok/error and ignores duplicate tool_use", () => {
    let items = replay([tool("t1"), tool("t1"), tool("t2", "command")]);
    expect(items).toHaveLength(2);
    items = applyEvent(items, { type: "tool_result", tool_use_id: "t1", ok: true, text: "File updated" });
    items = applyEvent(items, { type: "tool_result", tool_use_id: "t2", ok: false, text: "exit 1" });
    expect(items.map((i) => (i.kind === "tool" ? i.status : ""))).toEqual(["ok", "error"]);
  });

  it("a failed turn becomes a friendly error item and settles running tools", () => {
    const items = replay([
      tool("t1"),
      { type: "done", ok: false, text: "Not logged in", duration_ms: 1, code: "not_logged_in" },
    ]);
    const last = items.at(-1) as Extract<Item, { kind: "error" }>;
    expect(last).toMatchObject({ kind: "error", code: "not_logged_in" });
    expect(items[0]).toMatchObject({ kind: "tool", status: "ok" });
  });

  it("a successful turn adds nothing visible beyond the text", () => {
    const items = replay([
      { type: "text", message_id: "m", text: "done" },
      { type: "done", ok: true, text: "done", duration_ms: 1, code: null },
    ]);
    expect(items).toHaveLength(1);
  });

  it("replays a stored conversation: user, tools, snapshot", () => {
    const items = replay([
      { type: "user", text: "Maak een knop", context: "screen HomeScreen" },
      { type: "init", session_id: "s", model: null },
      tool("t1"),
      { type: "tool_result", tool_use_id: "t1", ok: true, text: "" },
      { type: "text", message_id: "m", text: "Klaar" },
      { type: "snapshot", commit: "abc", label: "Claude: Maak een knop" },
    ]);
    expect(items.map((i) => i.kind)).toEqual(["user", "tool", "assistant", "snapshot"]);
    expect(items[0]).toMatchObject({ context: "screen HomeScreen" });
  });

  it("shows Studio status lines in order and keeps them in history", () => {
    const items = replay([
      { type: "user", text: "make it bigger", context: null },
      { type: "status", code: "scope", detail: "small|HomeScreen" },
      { type: "status", code: "checking", detail: null },
      { type: "status", code: "rolled_back", detail: null },
    ]);
    expect(items.map((i) => i.kind)).toEqual(["user", "status", "status", "status"]);
    expect(items[1]).toMatchObject({ kind: "status", code: "scope", detail: "small|HomeScreen" });
  });
});
