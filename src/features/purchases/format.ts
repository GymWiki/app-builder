import type { ProductPeriod, PurchaseProduct } from "@/lib/types";

/** Alleen voor de uitleg in Studio (de basisprijs). In de app zelf komt de prijs altijd uit de winkel. */
export function money(amount: number, currency: string, locale: string): string {
  try {
    return new Intl.NumberFormat(locale, { style: "currency", currency }).format(amount);
  } catch {
    return `${amount.toFixed(2)} ${currency}`;
  }
}

export const PERIODS: ProductPeriod[] = ["week", "month", "year"];

export function emptyProduct(): PurchaseProduct {
  return {
    id: "",
    kind: "oneTime",
    name: "",
    description: "",
    basePrice: 0,
    currency: "EUR",
    period: null,
    trialDays: null,
    entitlement: "pro",
    status: { apple: "concept", google: "concept" },
  };
}

/** Is dit product al in de app of een winkel (dan ligt het ID vast)? */
export function isLocked(p: PurchaseProduct): boolean {
  return p.status.apple !== "concept" || p.status.google !== "concept";
}

/** Welke plekken in de app horen bij de unlock van dit product? */
export function unlocksOf(p: PurchaseProduct, points: { name: string; entitlement: string; kind: string }[]) {
  return points.filter((g) => g.entitlement === p.entitlement);
}
