import { useTranslation } from "react-i18next";
import type { GatePoint, PaywallSettings, PurchaseProduct } from "@/lib/types";
import { cn } from "@/lib/utils";
import { money, unlocksOf } from "./format";

/** Voorbeeld van de betaalmuur in de stijl van de app (themakleur). Prijzen hier zijn de basisprijs; in de app komen ze uit de winkel. */
export function PaywallPreview({
  paywall,
  products,
  points,
  seed,
  dark,
  locale,
}: {
  paywall: PaywallSettings;
  products: PurchaseProduct[];
  points: GatePoint[];
  seed: string;
  dark: boolean;
  locale: string;
}) {
  const { t } = useTranslation();
  const shown = (
    paywall.productIds.length
      ? paywall.productIds.map((id) => products.find((p) => p.id === id)).filter(Boolean)
      : products
  ) as PurchaseProduct[];
  const fg = dark ? "#f5f5f5" : "#171717";
  const bg = dark ? "#1c1b1f" : "#fffbfe";
  const sheet = paywall.layout === "sheet";
  const compact = paywall.layout === "compact";
  return (
    <div
      role="img"
      aria-label={t("sec.purchases.paywall.previewLabel")}
      className="bg-muted/50 grid place-items-center rounded-xl border p-4"
    >
      <div
        style={{ background: bg, color: fg, width: 280, minHeight: compact ? 300 : 460 }}
        className={cn(
          "relative flex flex-col gap-3 overflow-hidden border-4 border-neutral-800 p-4 shadow-lg",
          sheet ? "justify-end rounded-3xl" : "rounded-3xl",
        )}
      >
        <span aria-hidden className="absolute top-3 right-3 text-xs opacity-60">
          ✕
        </span>
        <p className="text-lg font-bold" style={{ color: seed }}>
          {paywall.headline || t("sec.purchases.paywall.headlineExample")}
        </p>
        {!compact && <p className="text-sm opacity-80">{paywall.body || t("sec.purchases.paywall.bodyExample")}</p>}
        <ul className="space-y-2">
          {shown.map((p) => {
            const featured = paywall.featured === p.id;
            return (
              <li
                key={p.id}
                className="rounded-xl border-2 p-2.5 text-sm"
                style={{ borderColor: featured ? seed : "#8884" }}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="font-semibold">{p.name}</span>
                  {featured && (
                    <span
                      className="rounded-full px-2 py-0.5 text-[10px] font-semibold text-white"
                      style={{ background: seed }}
                    >
                      {t("sec.purchases.paywall.featured")}
                    </span>
                  )}
                </div>
                <div className="text-xs opacity-80">
                  {t("sec.purchases.paywall.storePrice")}
                  {p.period ? ` / ${t(`sec.purchases.periods.${p.period}`)}` : ""}
                  {p.trialDays ? ` · ${t("sec.purchases.paywall.trial", { days: p.trialDays })}` : ""}
                </div>
                {!compact && unlocksOf(p, points).length > 0 && (
                  <div className="mt-1 text-[11px] opacity-70">
                    {unlocksOf(p, points)
                      .map((g) => g.name)
                      .join(" · ")}
                  </div>
                )}
                <div className="mt-1 text-[10px] opacity-50">
                  {t("sec.purchases.paywall.baseHint", { price: money(p.basePrice, p.currency, locale) })}
                </div>
              </li>
            );
          })}
          {shown.length === 0 && <li className="text-xs opacity-60">{t("sec.purchases.paywall.noProducts")}</li>}
        </ul>
        <div className="mt-auto space-y-2 text-center">
          <div className="rounded-full py-2 text-sm font-semibold text-white" style={{ background: seed }}>
            {paywall.cta || t("sec.purchases.paywall.ctaExample")}
          </div>
          <div className="text-xs underline opacity-80">{t("sec.purchases.paywall.restore")}</div>
          <div className="text-[10px] opacity-60">{t("sec.purchases.paywall.legal")}</div>
        </div>
      </div>
    </div>
  );
}
