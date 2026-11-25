/** Spiegel van het manifestformaat in src-tauri/src/manifest.rs en de statussen in vault.rs (alleen metadata, nooit waarden). */

export type Scope = "global" | "project";
export type FieldKind = "text" | "password" | "file" | "select";

export interface Validation {
  regex?: string;
  bestandstype?: string[];
  minLengte?: number;
  bevat?: string;
  json?: string[];
  geenServiceRol?: boolean;
}

export interface ManifestField {
  id: string;
  label: string;
  type: FieldKind;
  geheim: boolean;
  verplicht: boolean;
  scope: Scope;
  validatie: Validation;
  hulptekst: string;
  /** Plek voor een latere visuele gids; nu altijd leeg. */
  guideId?: string | null;
  envVar?: string;
  afgeleid?: string[];
  standaard?: string;
  opties?: string[];
}

export interface Manifest {
  id: string;
  naam: string;
  categorie: "stores" | "database" | "services" | string;
  beschrijving: string;
  icoon: string;
  scope: Scope;
  velden: ManifestField[];
  testVerbinding?: string;
  checklistItems: string[];
  afhankelijkheden?: string[];
  acties?: string[];
}

export interface FieldStatus {
  id: string;
  present: boolean;
  public: boolean;
  scope: Scope;
  fileName: string | null;
  derived: Record<string, string>;
  setAt: string | null;
  lastUsed: string | null;
  usedBy: string;
}

export type IntegrationState = "off" | "missing" | "incomplete" | "complete" | "warning";

export interface IntegrationStatus {
  id: string;
  enabled: boolean;
  state: IntegrationState;
  missing: string[];
  fields: FieldStatus[];
  lastTested: string | null;
  lastTestOk: boolean | null;
  lastTestCode: string | null;
  expiresAt: string | null;
  expiresInDays: number | null;
  dependenciesOff: string[];
}

export interface IntegrationOverview {
  manifests: Manifest[];
  statuses: IntegrationStatus[];
}

export interface TestOutcome {
  ok: boolean;
  code: string;
  detail: string | null;
  expiresAt: string | null;
}

export interface Leak {
  file: string;
  line: number;
  kind: string;
  tracked: boolean;
  movable: boolean;
}

/** Een afgewezen waarde: code (+ detail) die de UI vertaalt, zoals de backend `rejected`-fouten opbouwt. */
export interface Rejection {
  code: string;
  detail?: string;
}

/** `rejected`-fout van de backend (`message` = "code" of "code:detail"). */
export function parseRejection(message: string): Rejection {
  const [code = "", ...rest] = message.split(":");
  return { code, detail: rest.length ? rest.join(":") : undefined };
}

function jwtRole(token: string): string | null {
  try {
    const payload = token.split(".")[1];
    if (!payload) return null;
    const json = atob(payload.replace(/-/g, "+").replace(/_/g, "/"));
    const role = (JSON.parse(json) as { role?: unknown }).role;
    return typeof role === "string" ? role : null;
  } catch {
    return null;
  }
}

/** Directe controle tijdens het typen. De backend controleert opnieuw en blijft leidend. */
export function validateText(f: ManifestField, raw: string): Rejection | null {
  const v = raw.trim();
  if (!v) return { code: "empty" };
  if (/[\r\n]/.test(v)) return { code: "multiline" };
  const min = f.validatie.minLengte;
  if (min && [...v].length < min) return { code: "too_short", detail: String(min) };
  if (f.type === "select" && !(f.opties ?? []).includes(v)) return { code: "not_an_option" };
  if (f.validatie.regex && !new RegExp(f.validatie.regex).test(v)) return { code: "pattern" };
  if (f.validatie.geenServiceRol && (jwtRole(v) === "service_role" || v.startsWith("sb_secret_"))) {
    return { code: "secret_in_public_field" };
  }
  return null;
}

function jsonPath(v: unknown, path: string): unknown {
  return path
    .split(".")
    .reduce<unknown>((cur, k) => (cur && typeof cur === "object" ? (cur as Record<string, unknown>)[k] : undefined), v);
}

export interface FileCheck {
  rejection: Rejection | null;
  derived: Record<string, string>;
}

export function validateFile(f: ManifestField, name: string, text: string | null, size: number): FileCheck {
  const fail = (code: string, detail?: string): FileCheck => ({ rejection: { code, detail }, derived: {} });
  if (size === 0) return fail("empty");
  if (size > 512 * 1024) return fail("too_large");
  const types = f.validatie.bestandstype;
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
  if (types && !types.includes(ext)) return fail("file_type", types.join(", "));
  const derived: Record<string, string> = {};
  if (f.validatie.bevat || f.validatie.json) {
    if (text === null) return fail("not_text");
    if (f.validatie.bevat && !text.includes(f.validatie.bevat)) return fail("content", f.validatie.bevat);
    if (f.validatie.json) {
      let parsed: unknown;
      try {
        parsed = JSON.parse(text);
      } catch {
        return fail("json_invalid");
      }
      for (const rule of f.validatie.json) {
        const [path, want] = rule.includes("=") ? (rule.split("=") as [string, string]) : [rule, undefined];
        const got = jsonPath(parsed, path);
        if (got === undefined) return fail("json_missing", path);
        if (want !== undefined && got !== want) return fail("json_wrong", path);
      }
      for (const p of f.afgeleid ?? []) {
        const val = jsonPath(parsed, p);
        if (typeof val === "string") derived[p.split(".").pop()!] = val;
      }
    }
  }
  return { rejection: null, derived };
}

export function manifestOf(overview: IntegrationOverview | undefined, id: string): Manifest | undefined {
  return overview?.manifests.find((m) => m.id === id);
}

/** Standaard-uitleg: gebruikt voor de UI-teksten bij een afwijzing. */
export const REJECTION_CODES = [
  "empty",
  "multiline",
  "too_short",
  "not_an_option",
  "pattern",
  "secret_in_public_field",
  "file_type",
  "content",
  "json_invalid",
  "json_missing",
  "json_wrong",
  "too_large",
  "not_text",
  "passphrase_short",
  "backup_unreadable",
  "vault_has_value",
] as const;

export const CATEGORIES = ["stores", "database", "services"] as const;
