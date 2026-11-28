/** Mappen om schermen in te ordenen. Alleen ordening in Studio: de code van de app verandert er niet door. */
export interface Folder {
  id: string;
  name: string;
  parent: string | null;
}
export interface FolderState {
  folders: Folder[];
  /** scherm (klassenaam) → map; schermen zonder map staan los in de boom. */
  placed: Record<string, string>;
  /** ingeklapte mappen (standaard is alles open). */
  closed: string[];
}

export const EMPTY: FolderState = { folders: [], placed: {}, closed: [] };
const key = (projectId: string) => `studio-screens-tree:${projectId}`;

export function loadFolders(projectId: string): FolderState {
  try {
    const raw = localStorage.getItem(key(projectId));
    if (!raw) return EMPTY;
    const v = JSON.parse(raw) as Partial<FolderState>;
    return {
      folders: Array.isArray(v.folders) ? v.folders.filter((f) => f && typeof f.id === "string") : [],
      placed: v.placed && typeof v.placed === "object" ? v.placed : {},
      closed: Array.isArray(v.closed) ? v.closed : [],
    };
  } catch {
    return EMPTY;
  }
}

export function saveFolders(projectId: string, s: FolderState): void {
  try {
    localStorage.setItem(key(projectId), JSON.stringify(s));
  } catch {
    /* opslag niet beschikbaar: de indeling geldt dan alleen tijdens deze sessie */
  }
}

let counter = 0;
const newId = () => `f${Date.now().toString(36)}${(counter++).toString(36)}`;

export function addFolder(s: FolderState, name: string, parent: string | null): FolderState {
  const clean = name.trim();
  if (!clean) return s;
  return { ...s, folders: [...s.folders, { id: newId(), name: clean, parent }] };
}

export function renameFolder(s: FolderState, id: string, name: string): FolderState {
  const clean = name.trim();
  if (!clean) return s;
  return { ...s, folders: s.folders.map((f) => (f.id === id ? { ...f, name: clean } : f)) };
}

/** Verwijdert de map; wat erin zat gaat een niveau omhoog (er verdwijnt nooit een scherm). */
export function deleteFolder(s: FolderState, id: string): FolderState {
  const target = s.folders.find((f) => f.id === id);
  if (!target) return s;
  const placed: Record<string, string> = {};
  for (const [k, v] of Object.entries(s.placed)) {
    if (v !== id) placed[k] = v;
    else if (target.parent) placed[k] = target.parent;
  }
  return {
    folders: s.folders.filter((f) => f.id !== id).map((f) => (f.parent === id ? { ...f, parent: target.parent } : f)),
    placed,
    closed: s.closed.filter((c) => c !== id),
  };
}

export function moveScreen(s: FolderState, screen: string, folder: string | null): FolderState {
  const placed = { ...s.placed };
  if (folder) placed[screen] = folder;
  else delete placed[screen];
  return { ...s, placed };
}

/** Is `maybe` gelijk aan of onderdeel van `id`? (om lussen bij verplaatsen te voorkomen) */
export function isInside(s: FolderState, maybe: string | null, id: string): boolean {
  let cur = maybe;
  for (let i = 0; cur && i < 100; i++) {
    if (cur === id) return true;
    cur = s.folders.find((f) => f.id === cur)?.parent ?? null;
  }
  return false;
}

export function moveFolder(s: FolderState, id: string, parent: string | null): FolderState {
  if (isInside(s, parent, id)) return s;
  return { ...s, folders: s.folders.map((f) => (f.id === id ? { ...f, parent } : f)) };
}

export function toggleFolder(s: FolderState, id: string): FolderState {
  return { ...s, closed: s.closed.includes(id) ? s.closed.filter((c) => c !== id) : [...s.closed, id] };
}

/** Haalt verwijzingen weg naar schermen of mappen die niet meer bestaan. */
export function prune(s: FolderState, screens: string[]): FolderState {
  const ids = new Set(s.folders.map((f) => f.id));
  const known = new Set(screens);
  const placed = Object.fromEntries(Object.entries(s.placed).filter(([k, v]) => known.has(k) && ids.has(v)));
  return Object.keys(placed).length === Object.keys(s.placed).length ? s : { ...s, placed };
}

export type TreeItem =
  | { kind: "folder"; folder: Folder; depth: number; open: boolean; count: number }
  | { kind: "screen"; class: string; depth: number };

/** Platte lijst om te tonen: mappen eerst (op naam), dan de schermen in de volgorde die ze hadden. */
export function flatten(s: FolderState, screens: string[]): TreeItem[] {
  const out: TreeItem[] = [];
  const countIn = (id: string): number =>
    screens.filter((c) => s.placed[c] === id).length +
    s.folders.filter((f) => f.parent === id).reduce((n, f) => n + countIn(f.id), 0);
  const walk = (parent: string | null, depth: number) => {
    const subs = s.folders.filter((f) => f.parent === parent).sort((a, b) => a.name.localeCompare(b.name));
    for (const f of subs) {
      const open = !s.closed.includes(f.id);
      out.push({ kind: "folder", folder: f, depth, open, count: countIn(f.id) });
      if (open) walk(f.id, depth + 1);
    }
    for (const c of screens) if ((s.placed[c] ?? null) === parent) out.push({ kind: "screen", class: c, depth });
  };
  walk(null, 0);
  return out;
}
