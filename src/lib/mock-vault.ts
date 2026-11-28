/** Gesimuleerde kluis voor de browser-demo en de e2e-tests. Zelfde regels als de Rust-kant (manifest, validatie, status). */
import apple from "../../integrations/apple.json";
import firebase from "../../integrations/firebase.json";
import github from "../../integrations/github.json";
import googlePlay from "../../integrations/google_play.json";
import localDb from "../../integrations/local_db.json";
import supabase from "../../integrations/supabase.json";
import vercel from "../../integrations/vercel.json";
import purchases from "../../integrations/purchases.json";
import revenuecat from "../../integrations/revenuecat.json";
import { AppError } from "./errors";
import {
  validateFile,
  validateText,
  type IntegrationStatus,
  type Leak,
  type Manifest,
  type TestOutcome,
} from "./integrations";

const MANIFESTS = [
  apple,
  googlePlay,
  supabase,
  firebase,
  localDb,
  github,
  vercel,
  purchases,
  revenuecat,
] as unknown as Manifest[];
const by = (id: string) => {
  const m = MANIFESTS.find((x) => x.id === id);
  if (!m) throw new AppError("not_found", `integration ${id}`);
  return m;
};

interface Stored {
  kind: "text" | "file";
  value: string;
  fileName: string;
  derived: Record<string, string>;
  setAt: string;
  lastUsed: string | null;
  usedBy: string;
}
const values = new Map<string, Stored>();
const enabled = new Set<string>();
const tests = new Map<string, { at: string; ok: boolean; code: string }>();
const BASELINE = ["google_play"];

const keyOf = (projectId: string, integration: string, field: string) => {
  const f = by(integration).velden.find((x) => x.id === field);
  if (!f) throw new AppError("not_found", `field ${integration}.${field}`);
  return `${f.scope === "global" ? "global" : projectId}|${integration}|${field}`;
};
const rejected = (code: string, detail?: string) => new AppError("rejected", detail ? `${code}:${detail}` : code);

export const vaultHas = (projectId: string, integration: string, field: string) =>
  values.has(keyOf(projectId, integration, field));

function status(projectId: string, m: Manifest): IntegrationStatus {
  const on = enabled.has(`${projectId}|${m.id}`);
  const fields = m.velden.map((f) => {
    const s = values.get(keyOf(projectId, m.id, f.id));
    return {
      id: f.id,
      present: !!s,
      public: !f.geheim,
      scope: f.scope,
      fileName: s?.fileName || null,
      derived: s?.derived ?? {},
      setAt: s?.setAt ?? null,
      lastUsed: s?.lastUsed ?? null,
      usedBy: s?.usedBy ?? "",
    };
  });
  const required = m.velden.filter((f) => f.verplicht);
  const missing = required.filter((f) => !vaultHas(projectId, m.id, f.id)).map((f) => f.id);
  const t = tests.get(`${projectId}|${m.id}`);
  const state = !on
    ? "off"
    : required.length > 0 && missing.length === required.length
      ? "missing"
      : missing.length
        ? "incomplete"
        : t && !t.ok
          ? "warning"
          : "complete";
  return {
    id: m.id,
    enabled: on,
    state,
    missing,
    fields,
    lastTested: t?.at ?? null,
    lastTestOk: t?.ok ?? null,
    lastTestCode: t?.code ?? null,
    expiresAt: null,
    expiresInDays: null,
    dependenciesOff: (m.afhankelijkheden ?? []).filter((d) => !enabled.has(`${projectId}|${d}`)),
  };
}

/** Ontbrekende verplichte geheimen, zoals `Vault::missing_secrets` (geactiveerd of verplicht voor Android). */
export function vaultMissing(projectId: string) {
  const out: { integration: string; field: string; enabled: boolean; items: string[] }[] = [];
  for (const m of MANIFESTS) {
    const on = enabled.has(`${projectId}|${m.id}`);
    if (!on && !BASELINE.includes(m.id)) continue;
    for (const f of m.velden.filter((x) => x.verplicht && !vaultHas(projectId, m.id, x.id))) {
      out.push({ integration: m.id, field: f.id, enabled: on, items: m.checklistItems });
    }
  }
  return out;
}

export function vaultSetKeystore(projectId: string) {
  const now = new Date().toISOString();
  const put = (field: string, kind: "text" | "file", value: string, fileName = "") =>
    values.set(keyOf(projectId, "google_play", field), {
      kind,
      value,
      fileName,
      derived: {},
      setAt: now,
      lastUsed: null,
      usedBy: "",
    });
  put("keystore", "file", "binary", "upload-keystore.jks");
  put("keystore_password", "text", "generated-password-1234567890");
  put("alias", "text", "upload");
}

/** Demo: de sleutels die "Aankopen toepassen" nodig heeft alvast in de kluis (nep-waarden). */
export function vaultPreloadForPurchases(projectId: string) {
  const now = new Date().toISOString();
  for (const [i, f, kind] of [
    ["google_play", "service_account", "file"],
    ["apple", "private_key", "file"],
    ["revenuecat", "public_sdk_key", "text"],
  ] as const)
    values.set(keyOf(projectId, i, f), {
      kind,
      value: "demo",
      fileName: kind === "file" ? "demo-key" : "",
      derived: {},
      setAt: now,
      lastUsed: null,
      usedBy: "",
    });
}

const nowIso = () => new Date().toISOString();

export function mockLeaks(): Leak[] {
  return ((globalThis as { __studioMockLeaks?: Leak[] }).__studioMockLeaks ?? []).slice();
}

/** De oude vaste geheimnamen (Data- en Accounts-pagina) horen bij dezelfde kluis als de Geheimen-pagina. */
const LEGACY: Record<string, [string, string, boolean]> = {
  supabase_token: ["supabase", "access_token", false],
  android_keystore_password: ["google_play", "keystore_password", false],
  play_service_account_json: ["google_play", "service_account", true],
};

export async function vaultInvoke(
  cmd: string,
  a: Record<string, unknown>,
): Promise<{ handled: boolean; value?: unknown }> {
  const pid = a.projectId as string;
  const ok = (value?: unknown) => ({ handled: true, value });
  if (cmd === "secret_set" || cmd === "secret_has" || cmd === "secret_delete") {
    const m = LEGACY[a.name as string];
    if (!m) throw new AppError("invalid", `unknown secret ${String(a.name)}`);
    const [integration, field, file] = m;
    if (cmd === "secret_has") return ok(vaultHas(pid, integration, field));
    if (cmd === "secret_delete") return vaultInvoke("secret_field_delete", { projectId: pid, integration, field });
    const value = a.value as string;
    return file
      ? vaultInvoke("secret_field_set_file", {
          projectId: pid,
          integration,
          field,
          fileName: "service-account.json",
          dataB64: btoa(value),
        })
      : vaultInvoke("secret_field_set_text", { projectId: pid, integration, field, value });
  }
  switch (cmd) {
    case "integration_list":
      return ok({ manifests: MANIFESTS, statuses: MANIFESTS.map((m) => status(pid, m)) });
    case "integration_set_enabled": {
      const id = a.id as string;
      const m = by(id);
      if (a.enabled) enabled.add(`${pid}|${id}`);
      else {
        enabled.delete(`${pid}|${id}`);
        if (a.deleteSecrets)
          for (const f of m.velden.filter((x) => x.scope === "project")) values.delete(keyOf(pid, id, f.id));
      }
      return ok(MANIFESTS.map((x) => status(pid, x)));
    }
    case "secret_field_set_text": {
      const m = by(a.integration as string);
      const f = m.velden.find((x) => x.id === a.field)!;
      const r = validateText(f, a.value as string);
      if (r) throw rejected(r.code, r.detail);
      values.set(keyOf(pid, m.id, f.id), {
        kind: "text",
        value: (a.value as string).trim(),
        fileName: "",
        derived: {},
        setAt: nowIso(),
        lastUsed: null,
        usedBy: "",
      });
      tests.delete(`${pid}|${m.id}`);
      return ok();
    }
    case "secret_field_set_file": {
      const m = by(a.integration as string);
      const f = m.velden.find((x) => x.id === a.field)!;
      const bytes = atob(a.dataB64 as string);
      let text: string | null;
      try {
        text = decodeURIComponent(escape(bytes));
      } catch {
        text = null;
      }
      const c = validateFile(f, a.fileName as string, text, bytes.length);
      if (c.rejection) throw rejected(c.rejection.code, c.rejection.detail);
      values.set(keyOf(pid, m.id, f.id), {
        kind: "file",
        value: "file",
        fileName: a.fileName as string,
        derived: c.derived,
        setAt: nowIso(),
        lastUsed: null,
        usedBy: "",
      });
      tests.delete(`${pid}|${m.id}`);
      return ok({ derived: c.derived });
    }
    case "secret_field_delete":
      values.delete(keyOf(pid, a.integration as string, a.field as string));
      tests.delete(`${pid}|${a.integration as string}`);
      return ok();
    case "secret_field_reveal": {
      const s = values.get(keyOf(pid, a.integration as string, a.field as string));
      if (!s || s.kind === "file") throw new AppError("not_found", "value");
      return ok(s.value);
    }
    case "secret_test": {
      await new Promise((r) => setTimeout(r, 400));
      const m = by(a.integration as string);
      const missing = m.velden.filter((f) => f.verplicht && !vaultHas(pid, m.id, f.id));
      let out: TestOutcome;
      if (missing.length) out = { ok: false, code: "incomplete", detail: null, expiresAt: null };
      else if (["supabase", "github", "vercel"].includes(m.testVerbinding ?? "")) {
        const tokenField = m.testVerbinding === "supabase" ? "publishable_key" : "token";
        const bad = values.get(keyOf(pid, m.id, tokenField))?.value.includes("bad");
        out = bad
          ? { ok: false, code: "invalid_credentials", detail: null, expiresAt: null }
          : { ok: true, code: "ok", detail: m.testVerbinding === "github" ? "octo-cat" : null, expiresAt: null };
      } else out = { ok: true, code: "checked_locally", detail: null, expiresAt: null };
      tests.set(`${pid}|${m.id}`, { at: nowIso(), ok: out.ok, code: out.code });
      for (const f of m.velden) {
        const s = values.get(keyOf(pid, m.id, f.id));
        if (s) Object.assign(s, { lastUsed: nowIso(), usedBy: "test" });
      }
      return ok(out);
    }
    case "vault_export": {
      if ((a.passphrase as string).length < 8) throw rejected("passphrase_short", "8");
      return ok(btoa(JSON.stringify({ magic: "STUDIOVAULT1", pass: a.passphrase, entries: [...values.entries()] })));
    }
    case "vault_import": {
      let data: { magic?: string; pass?: string; entries?: [string, Stored][] };
      try {
        data = JSON.parse(atob(a.dataB64 as string));
      } catch {
        throw rejected("backup_unreadable");
      }
      if (data.magic !== "STUDIOVAULT1" || data.pass !== a.passphrase) throw rejected("backup_unreadable");
      for (const [k, v] of data.entries ?? []) values.set(k, v);
      return ok((data.entries ?? []).length);
    }
    case "leaks_scan":
      return ok(mockLeaks());
    case "leak_move_to_vault": {
      const list = mockLeaks();
      (globalThis as { __studioMockLeaks?: Leak[] }).__studioMockLeaks = list.filter((l) => l.file !== a.file);
      return ok(["apple", "private_key"]);
    }
    case "leak_fix_prompt":
      return ok(`Remove the secret from ${a.file as string}.`);
    default:
      return { handled: false };
  }
}
