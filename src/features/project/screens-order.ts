import type { ScreenGraph } from "@/lib/types";

interface Row {
  class: string;
  file: string;
  line: number;
  depth: number;
}

/** Zet de graaf om naar een ingesprongen boom vanaf het beginscherm (elk scherm één keer). */
export function treeRows(g: ScreenGraph): { rows: Row[]; unlinked: Row[] } {
  const by = new Map(g.nodes.map((n) => [n.class, n]));
  const seen = new Set<string>();
  const rows: Row[] = [];
  const visit = (name: string, depth: number) => {
    const n = by.get(name);
    if (!n || seen.has(name)) return;
    seen.add(name);
    rows.push({ class: n.class, file: n.file, line: n.line, depth });
    n.children.forEach((c) => visit(c, depth + 1));
  };
  if (g.home) visit(g.home, 0);
  const unlinked = g.nodes
    .filter((n) => !seen.has(n.class))
    .map((n) => ({ class: n.class, file: n.file, line: n.line, depth: 0 }));
  return { rows, unlinked };
}
