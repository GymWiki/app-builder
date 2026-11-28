/** Gesimuleerde Aankopen-backend voor de demo en de e2e-tests. Zelfde regels als `purchases.rs` / `purchases_flow.rs`. */
import { vaultHas } from "./mock-vault";
import type {
  ApplyPlan,
  GatePoint,
  ProductKind,
  ProductPeriod,
  PurchaseProduct,
  Purchases,
  PurchasesView,
  StoreStatus,
  StoreTask,
} from "./types";

const order: StoreStatus[] = ["concept", "inApp", "created", "inReview", "live"];
const rank = (s: StoreStatus) => order.indexOf(s);

export const emptyPurchases = (): Purchases => ({
  schema: 1,
  provider: "revenueCat",
  products: [],
  paywallDescription: "",
  gating: { points: [], approved: false },
  paywall: {
    placement: "lockedFeature",
    afterUses: 3,
    layout: "fullScreen",
    style: "",
    headline: "",
    body: "",
    cta: "",
    productIds: [],
    featured: null,
  },
  appliedVersion: 0,
  applied: null,
  confirmations: {},
  apple: true,
  google: true,
});

const locked = (p: PurchaseProduct) => rank(p.status.apple) >= 1 || rank(p.status.google) >= 1;

export function suggestId(name: string, kind: ProductKind, period: ProductPeriod | null): string {
  let s = name
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_|_$/g, "");
  if (!s || !/^[a-z]/.test(s)) s = `product_${s}`.replace(/_$/, "");
  if (kind === "subscription") s += `_${period === "week" ? "weekly" : period === "month" ? "monthly" : "yearly"}`;
  return s;
}

export function validate(p: Purchases): { product: string; code: string }[] {
  const out: { product: string; code: string }[] = [];
  const seen = new Set<string>();
  for (const pr of p.products) {
    const bad = (code: string) => out.push({ product: pr.id, code });
    if (!/^[a-z][a-z0-9_]{2,59}$/.test(pr.id)) bad("id_invalid");
    if (seen.has(pr.id)) bad("id_duplicate");
    seen.add(pr.id);
    if (!pr.name.trim()) bad("name_missing");
    if (!(pr.basePrice > 0)) bad("price_missing");
    if (!/^[A-Z]{3}$/.test(pr.currency)) bad("currency_invalid");
    if (pr.kind === "subscription" && !pr.period) bad("period_missing");
    if (pr.kind !== "subscription" && (pr.period || pr.trialDays)) bad("period_only_for_subscriptions");
    if (pr.kind !== "consumable" && !pr.entitlement.trim()) bad("entitlement_missing");
  }
  return out;
}

interface Applied {
  products: PurchaseProduct[];
  gating: Purchases["gating"];
  paywall: Purchases["paywall"];
  provider: Purchases["provider"];
}

export function plan(p: Purchases): ApplyPlan {
  const was = (p.applied as Applied | null) ?? {
    products: [],
    gating: { points: [], approved: false },
    paywall: emptyPurchases().paywall,
    provider: p.provider,
  };
  const out: ApplyPlan = {
    firstTime: !p.applied,
    create: [],
    update: [],
    remove: [],
    gatePointsAdded: 0,
    gatePointsRemoved: 0,
    paywallChanged: false,
    providerChanged: false,
    risks: [],
  };
  for (const pr of p.products) {
    const old = was.products.find((o) => o.id === pr.id);
    if (!old) {
      out.create.push(pr.id);
      continue;
    }
    const keys = [
      "name",
      "description",
      "basePrice",
      "currency",
      "period",
      "trialDays",
      "entitlement",
      "kind",
    ] as const;
    if (keys.some((k) => old[k] !== pr[k])) out.update.push(pr.id);
    const live = rank(old.status.apple) >= 2 || rank(old.status.google) >= 2;
    if (live && pr.kind === "subscription" && pr.basePrice > old.basePrice)
      out.risks.push({ code: "price_increase", product: pr.id });
    if (live && old.entitlement !== pr.entitlement) out.risks.push({ code: "entitlement_changed", product: pr.id });
  }
  for (const old of was.products) {
    if (!p.products.some((x) => x.id === old.id)) {
      out.remove.push(old.id);
      if (rank(old.status.apple) >= 3 || rank(old.status.google) >= 3)
        out.risks.push({ code: "remove_with_buyers", product: old.id });
    }
  }
  const key = (g: GatePoint) => `${g.kind}|${g.name}|${g.entitlement}`;
  out.gatePointsAdded = p.gating.points.filter((g) => !was.gating.points.some((o) => key(o) === key(g))).length;
  out.gatePointsRemoved = was.gating.points.filter((o) => !p.gating.points.some((g) => key(g) === key(o))).length;
  out.paywallChanged =
    JSON.stringify(p.paywall) !== JSON.stringify(was.paywall) && !(!p.applied && p.products.length === 0);
  out.providerChanged = !!p.applied && p.provider !== was.provider;
  return out;
}

export function summary(pl: ApplyPlan): [string, number][] {
  const v: [string, number][] = [];
  if (pl.create.length) v.push(["create", pl.create.length]);
  if (pl.update.length) v.push(["update", pl.update.length]);
  if (pl.remove.length) v.push(["remove", pl.remove.length]);
  if (pl.gatePointsAdded) v.push(["lock", pl.gatePointsAdded]);
  if (pl.gatePointsRemoved) v.push(["unlock", pl.gatePointsRemoved]);
  if (pl.paywallChanged) v.push(["paywall", 1]);
  if (pl.providerChanged) v.push(["provider", 1]);
  return v;
}

export function missing(p: Purchases, projectId: string): { code: string; next: string }[] {
  const m: { code: string; next: string }[] = [];
  if (!p.products.length) m.push({ code: "no_products", next: "purchases:products" });
  if (validate(p).length) m.push({ code: "product_problems", next: "purchases:products" });
  if ((p.paywallDescription.trim() || p.gating.points.length) && !p.gating.approved)
    m.push({ code: "gating_not_approved", next: "purchases:gating" });
  if (p.google && !vaultHas(projectId, "google_play", "service_account"))
    m.push({ code: "secret_google", next: "secrets:google_play" });
  if (p.apple && !vaultHas(projectId, "apple", "private_key")) m.push({ code: "secret_apple", next: "secrets:apple" });
  if (p.provider === "revenueCat" && !vaultHas(projectId, "revenuecat", "public_sdk_key"))
    m.push({ code: "secret_revenuecat", next: "secrets:revenuecat" });
  return m;
}

export function tasks(p: Purchases, bundle: string): StoreTask[] {
  const out: StoreTask[] = [];
  for (const pr of p.products)
    for (const [store, on, st] of [
      ["apple", p.apple, pr.status.apple],
      ["google", p.google, pr.status.google],
    ] as const)
      if (on && rank(st) < 2)
        out.push({
          store,
          product: pr.id,
          fullId: `${bundle}.${pr.id}`,
          name: pr.name,
          kind: pr.kind,
          period: pr.period,
          basePrice: pr.basePrice,
          currency: pr.currency,
          trialDays: pr.trialDays,
        });
  return out;
}

export function finalize(p: Purchases) {
  for (const pr of p.products) {
    if (pr.status.apple === "concept") pr.status.apple = "inApp";
    if (pr.status.google === "concept") pr.status.google = "inApp";
  }
  p.applied = JSON.parse(
    JSON.stringify({ products: p.products, gating: p.gating, paywall: p.paywall, provider: p.provider }),
  );
  p.appliedVersion += 1;
}

export function view(p: Purchases, projectId: string, bundle: string, applying: boolean): PurchasesView {
  const pl = plan(p);
  return {
    purchases: p,
    plan: pl,
    summary: summary(pl),
    missing: missing(p, projectId),
    problems: validate(p),
    tasks: tasks(p, bundle),
    bundleId: bundle,
    applying,
  };
}

export { locked };
export { rank };
