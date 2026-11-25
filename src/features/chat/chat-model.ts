import type { ChatEvent, ToolKind } from "@/lib/types";

export type ToolStatus = "running" | "ok" | "error";

export type Item =
  | { kind: "user"; key: string; text: string; context: string | null }
  | { kind: "assistant"; key: string; messageId: string; text: string; live: boolean }
  | {
      kind: "tool";
      key: string;
      id: string;
      tool: ToolKind;
      summary: string;
      path: string | null;
      detail: string | null;
      status: ToolStatus;
      result: string | null;
    }
  | { kind: "snapshot"; key: string; label: string; commit: string }
  | { kind: "error"; key: string; code: string; message: string };

const next = (items: Item[]) => `i${items.length}`;

/** Zet één gebeurtenis om in de lijst weergave-items (zuivere functie; ook gebruikt om geschiedenis af te spelen). */
export function applyEvent(items: Item[], ev: ChatEvent): Item[] {
  switch (ev.type) {
    case "user":
      return [...items, { kind: "user", key: next(items), text: ev.text, context: ev.context }];

    case "text_delta": {
      const i = items.findIndex((x) => x.kind === "assistant" && x.live && x.messageId === ev.message_id);
      if (i >= 0) {
        const cur = items[i] as Extract<Item, { kind: "assistant" }>;
        return items.map((x, n) => (n === i ? { ...cur, text: cur.text + ev.text } : x));
      }
      return [...items, { kind: "assistant", key: next(items), messageId: ev.message_id, text: ev.text, live: true }];
    }

    case "text": {
      // De volledige tekst vervangt de live-weergave van hetzelfde bericht.
      const i = items.findIndex((x) => x.kind === "assistant" && x.live && x.messageId === ev.message_id);
      if (i >= 0) {
        return items.map((x, n) =>
          n === i ? { kind: "assistant", key: x.key, messageId: ev.message_id, text: ev.text, live: false } : x,
        );
      }
      return [...items, { kind: "assistant", key: next(items), messageId: ev.message_id, text: ev.text, live: false }];
    }

    case "tool_use":
      if (items.some((x) => x.kind === "tool" && x.id === ev.id)) return items;
      return [
        ...items,
        {
          kind: "tool",
          key: next(items),
          id: ev.id,
          tool: ev.kind,
          summary: ev.summary,
          path: ev.path,
          detail: ev.detail,
          status: "running",
          result: null,
        },
      ];

    case "tool_result":
      return items.map((x) =>
        x.kind === "tool" && x.id === ev.tool_use_id
          ? { ...x, status: ev.ok ? "ok" : "error", result: ev.text || null }
          : x,
      );

    case "snapshot":
      return [...items, { kind: "snapshot", key: next(items), label: ev.label, commit: ev.commit }];

    case "done":
      return ev.ok
        ? finishRunning(items)
        : [...finishRunning(items), { kind: "error", key: next(items), code: ev.code ?? "failed", message: ev.text }];

    case "error":
      return [...finishRunning(items), { kind: "error", key: next(items), code: ev.code, message: ev.message }];

    default:
      return items;
  }
}

/** Na het einde van een beurt: niets blijft op "bezig" staan en live-tekst wordt definitief. */
export function finishRunning(items: Item[]): Item[] {
  return items.map((x) => {
    if (x.kind === "tool" && x.status === "running") return { ...x, status: "ok" as const };
    if (x.kind === "assistant" && x.live) return { ...x, live: false };
    return x;
  });
}

export const replay = (events: ChatEvent[]): Item[] => events.reduce(applyEvent, [] as Item[]);
