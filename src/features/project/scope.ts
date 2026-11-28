import type { FolderState } from "./screens-folders";

/** Bereik van een chatopdracht. `null` (buiten dit bestand) = de huidige pagina, zoals altijd. */
export type Scope =
  | { kind: "app" }
  | { kind: "folder"; folderId: string; name: string; pages: string[] }
  | { kind: "pages"; pages: string[] };

/** De pagina's (klassenamen) waar een bereik over gaat. */
export function scopePages(scope: Scope, all: string[]): string[] {
  return scope.kind === "app" ? all : scope.pages.filter((p) => all.includes(p));
}

/** Alle pagina's in een map, ook in geneste mappen. */
export function folderPages(state: FolderState, folderId: string, all: string[]): string[] {
  const inside = new Set<string>([folderId]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const f of state.folders) {
      if (f.parent && inside.has(f.parent) && !inside.has(f.id)) {
        inside.add(f.id);
        grew = true;
      }
    }
  }
  return all.filter((p) => state.placed[p] && inside.has(state.placed[p]!));
}

/** Ctrl/Cmd-klik voegt toe of haalt weg; Shift-klik kiest alles tussen het anker en de aangeklikte pagina (in lijstvolgorde). */
export function toggleMulti(current: string[], clicked: string, opts: { shift: boolean; anchor: string | null; order: string[] }): string[] {
  if (opts.shift && opts.anchor && opts.order.includes(opts.anchor)) {
    const a = opts.order.indexOf(opts.anchor);
    const b = opts.order.indexOf(clicked);
    const [lo, hi] = a < b ? [a, b] : [b, a];
    const range = opts.order.slice(lo, hi + 1);
    return [...new Set([...current, ...range])];
  }
  return current.includes(clicked) ? current.filter((p) => p !== clicked) : [...current, clicked];
}

export type Strategy = "single" | "fanout" | "plan";
export interface Classification {
  breadth: "narrow" | "wide";
  difficulty: "simple" | "complex";
  strategy: Strategy;
}

/** Wijzigingen die dingen verplaatsen of de opbouw veranderen: breed én ingewikkeld. */
const COMPLEX =
  /\b(navigat\w*|architect\w*|data ?model\w*|gegevens\w*|database|schema|migrat\w*|restructur\w*|refactor\w*|rename\w*|hernoem\w*|verplaats\w*|move\w*|state management|login|log in|inlog\w*|account\w*|synchron\w*|offline|routing|routes?|dependenc\w*|package\w*|pakket\w*|nieuwe? (pagina|scherm)\w*|new (page|screen)s?)\b/i;

/** Breedte (aantal pagina's) en moeilijkheid (soort wijziging) zijn twee aparte signalen. */
export function classify(prompt: string, pageCount: number): Classification {
  const breadth = pageCount >= 3 ? "wide" : "narrow";
  const difficulty = COMPLEX.test(prompt) ? "complex" : "simple";
  const strategy: Strategy = breadth === "narrow" ? "single" : difficulty === "simple" ? "fanout" : "plan";
  return { breadth, difficulty, strategy };
}

export interface FanoutStep {
  kind: "shared" | "page";
  /** klassenaam bij `page` */
  page?: string;
}

/** Volgorde van een brede, eenvoudige opdracht: eerst het gedeelde deel (`core/`), dan één kleine taak per pagina. */
export function fanoutPlan(pages: string[]): FanoutStep[] {
  return [{ kind: "shared" }, ...pages.map((page) => ({ kind: "page" as const, page }))];
}
