/** Het plan (blueprint) van een nieuwe app, gelijk aan `studio/blueprint.json` (Rust: `blueprint.rs`). */
export interface BpPage {
  id: string;
  name: string;
  purpose: string;
  route: string;
  components: string[];
  models: string[];
  features: string[];
  links: string[];
}
export interface BpField {
  name: string;
  type: "text" | "int" | "double" | "bool" | "date" | "enum" | "ref";
  ref: string | null;
  values: string[];
}
export interface BpModel {
  id: string;
  name: string;
  fields: BpField[];
  sample: Record<string, unknown>[];
}
export interface BpFeature {
  id: string;
  title: string;
  capability: string;
  pages: string[];
  status: "now" | "later";
}
export interface Blueprint {
  version: 1;
  app: { name: string; purpose: string; audience: string; language: string; description: string };
  style: { card: string; seed: string; font: string; kit: string; logo: string | null };
  navigation: { kind: "tabs" | "menu" | "single"; order: string[] };
  pages: BpPage[];
  models: BpModel[];
  features: BpFeature[];
  integrations: { storage: "local" | "supabase" | "firebase"; auth: string[]; purchases: boolean; notifications: boolean };
  assumptions: { id: string; text: string; editable: boolean }[];
}

export const slug = (s: string, fallback = "page") => {
  const base = s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .replace(/^[0-9]+/, "")
    .slice(0, 24);
  return base || fallback;
};

const unique = (base: string, taken: string[]) => {
  let id = base;
  for (let n = 2; taken.includes(id); n++) id = `${base}_${n}`;
  return id;
};

/** Pagina toevoegen: eigen id en route, en ook in de navigatie. */
export function addPage(bp: Blueprint, name: string): Blueprint {
  const clean = name.trim().slice(0, 30);
  if (!clean || bp.pages.length >= 12) return bp;
  const id = unique(slug(clean), bp.pages.map((p) => p.id));
  const route = unique(`/${id}`, bp.pages.map((p) => p.route));
  const page: BpPage = { id, name: clean, purpose: "", route, components: [], models: [], features: [], links: [] };
  return { ...bp, pages: [...bp.pages, page], navigation: { ...bp.navigation, order: [...bp.navigation.order, id] } };
}

export function renamePage(bp: Blueprint, id: string, name: string): Blueprint {
  return { ...bp, pages: bp.pages.map((p) => (p.id === id ? { ...p, name: name.slice(0, 30) } : p)) };
}

export function setPurpose(bp: Blueprint, id: string, purpose: string): Blueprint {
  return { ...bp, pages: bp.pages.map((p) => (p.id === id ? { ...p, purpose: purpose.slice(0, 200) } : p)) };
}

/** Pagina verwijderen. De startpagina (route "/") en de laatste pagina blijven altijd bestaan; verwijzingen ernaar verdwijnen overal mee. */
export function removePage(bp: Blueprint, id: string): Blueprint {
  const page = bp.pages.find((p) => p.id === id);
  if (!page || page.route === "/" || bp.pages.length <= 1) return bp;
  return {
    ...bp,
    pages: bp.pages.filter((p) => p.id !== id).map((p) => ({ ...p, links: p.links.filter((l) => l !== id) })),
    navigation: { ...bp.navigation, order: bp.navigation.order.filter((o) => o !== id) },
    features: bp.features.map((f) => ({ ...f, pages: f.pages.filter((p) => p !== id) })),
  };
}

const move = <T,>(list: T[], i: number, dir: -1 | 1): T[] => {
  const j = i + dir;
  if (i < 0 || j < 0 || j >= list.length) return list;
  const next = [...list];
  [next[i], next[j]] = [next[j]!, next[i]!];
  return next;
};

export function movePage(bp: Blueprint, id: string, dir: -1 | 1): Blueprint {
  const i = bp.pages.findIndex((p) => p.id === id);
  const order = bp.pages.map((p) => p.id);
  const moved = move(bp.pages, i, dir);
  // de navigatie volgt de volgorde van de lijst (alleen pagina's die erin staan)
  const inNav = new Set(bp.navigation.order);
  return { ...bp, pages: moved, navigation: { ...bp.navigation, order: moved.map((p) => p.id).filter((x) => inNav.has(x) || !order.includes(x)) } };
}

export function setNavigation(bp: Blueprint, kind: Blueprint["navigation"]["kind"]): Blueprint {
  return { ...bp, navigation: { kind, order: bp.pages.map((p) => p.id) } };
}

export function setAssumption(bp: Blueprint, id: string, text: string): Blueprint {
  return { ...bp, assumptions: bp.assumptions.map((a) => (a.id === id ? { ...a, text: text.slice(0, 200) } : a)) };
}
export function addAssumption(bp: Blueprint, text: string): Blueprint {
  const t = text.trim().slice(0, 200);
  if (!t) return bp;
  return { ...bp, assumptions: [...bp.assumptions, { id: unique(`a${bp.assumptions.length + 1}`, bp.assumptions.map((a) => a.id)), text: t, editable: true }] };
}
export function removeAssumption(bp: Blueprint, id: string): Blueprint {
  return { ...bp, assumptions: bp.assumptions.filter((a) => a.id !== id) };
}

/** Inloggen of synchroniseren aan/uit: de koppelingen en de bijbehorende functie blijven gelijk, zodat het plan altijd klopt. */
export function setSignIn(bp: Blueprint, on: boolean): Blueprint {
  const rest = bp.features.filter((f) => f.capability !== "auth_email");
  return {
    ...bp,
    integrations: { ...bp.integrations, auth: on ? ["email"] : [] },
    features: on ? [...rest, { id: unique("sign_in", rest.map((f) => f.id)), title: "Sign in", capability: "auth_email", pages: [], status: "now" }] : rest,
  };
}
export function setSync(bp: Blueprint, provider: "supabase" | "firebase" | null): Blueprint {
  const rest = bp.features.filter((f) => !["cloud_sync_supabase", "cloud_sync_firebase"].includes(f.capability));
  return {
    ...bp,
    integrations: { ...bp.integrations, storage: provider ?? "local" },
    features: provider
      ? [...rest, { id: unique("sync", rest.map((f) => f.id)), title: "Sync between devices", capability: `cloud_sync_${provider}`, pages: [], status: "now" }]
      : rest,
  };
}
