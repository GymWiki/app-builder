import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, inputCls } from "@/features/sections/shared";
import { api } from "@/lib/ipc";
import type { ProductKind, PurchaseProduct } from "@/lib/types";
import { cn } from "@/lib/utils";
import { PERIODS, isLocked } from "./format";

const KINDS: ProductKind[] = ["oneTime", "subscription", "consumable"];
const CURRENCIES = ["EUR", "USD", "GBP", "CAD", "AUD", "CHF", "SEK", "NOK", "DKK", "JPY"];

/** Compact formulier voor één product. Het ID volgt de naam, tot het in de app of een winkel staat: dan ligt het vast. */
export function ProductDialog({
  open,
  initial,
  bundleId,
  taken,
  onClose,
  onSave,
}: {
  open: boolean;
  initial: PurchaseProduct;
  bundleId: string;
  taken: string[];
  onClose: () => void;
  onSave: (p: PurchaseProduct) => void;
}) {
  const { t } = useTranslation();
  const [p, setP] = useState(initial);
  const [price, setPrice] = useState(initial.basePrice ? String(initial.basePrice) : "");
  const [idTouched, setIdTouched] = useState(!!initial.id);
  const seq = useRef(0);
  const locked = isLocked(initial);

  // Het ID volgt naam, soort en periode (op één plek bepaald in de backend), zolang de gebruiker het niet zelf aanpast.
  useEffect(() => {
    if (!open || idTouched || locked || !p.name.trim()) return;
    const n = ++seq.current;
    void api
      .purchasesSuggestId(p.name, p.kind, p.period)
      .then((id) => n === seq.current && setP((x) => ({ ...x, id })))
      .catch(() => undefined);
  }, [open, idTouched, locked, p.name, p.kind, p.period]);

  const set = (patch: Partial<PurchaseProduct>) => setP((x) => ({ ...x, ...patch }));
  const sub = p.kind === "subscription";
  const priceNum = Number(price.replace(",", "."));
  const idOk = /^[a-z][a-z0-9_]{2,59}$/.test(p.id) && (!taken.includes(p.id) || initial.id === p.id);
  const valid =
    p.name.trim() && priceNum > 0 && idOk && (!sub || p.period) && (p.kind === "consumable" || p.entitlement.trim());

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>
            {t(initial.id ? "sec.purchases.product.editTitle" : "sec.purchases.product.addTitle")}
          </DialogTitle>
          <DialogDescription>{t("sec.purchases.product.intro")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <Field label={t("sec.purchases.product.kind")}>
            <div role="radiogroup" aria-label={t("sec.purchases.product.kind")} className="grid gap-2 sm:grid-cols-3">
              {KINDS.map((k) => (
                <button
                  key={k}
                  type="button"
                  role="radio"
                  aria-checked={p.kind === k}
                  disabled={locked}
                  onClick={() =>
                    set({
                      kind: k,
                      period: k === "subscription" ? (p.period ?? "year") : null,
                      trialDays: k === "subscription" ? p.trialDays : null,
                    })
                  }
                  className={cn(
                    "rounded-lg border p-2.5 text-left transition-colors disabled:opacity-60",
                    p.kind === k ? "border-primary bg-accent/60" : "hover:bg-muted/60",
                  )}
                >
                  <span className="t-body block font-medium">{t(`sec.purchases.kinds.${k}`)}</span>
                  <span className="t-small text-muted-foreground block">{t(`sec.purchases.kindHelp.${k}`)}</span>
                </button>
              ))}
            </div>
          </Field>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label={t("sec.purchases.product.name")} htmlFor="p-name">
              <input
                id="p-name"
                value={p.name}
                onChange={(e) => set({ name: e.target.value })}
                className={inputCls}
                placeholder={t("sec.purchases.product.namePlaceholder")}
              />
            </Field>
            <Field
              label={t("sec.purchases.product.unlock")}
              help={t("sec.purchases.product.unlockHelp")}
              htmlFor="p-unlock"
            >
              <input
                id="p-unlock"
                value={p.entitlement}
                disabled={p.kind === "consumable"}
                onChange={(e) => set({ entitlement: e.target.value.trim().toLowerCase() })}
                className={inputCls}
              />
            </Field>
          </div>
          <Field label={t("sec.purchases.product.description")} htmlFor="p-desc">
            <input
              id="p-desc"
              value={p.description}
              onChange={(e) => set({ description: e.target.value })}
              className={inputCls}
            />
          </Field>
          <div className="grid gap-4 sm:grid-cols-3">
            <Field
              label={t("sec.purchases.product.price")}
              help={t("sec.purchases.product.priceHelp")}
              htmlFor="p-price"
            >
              <input
                id="p-price"
                inputMode="decimal"
                value={price}
                onChange={(e) => {
                  setPrice(e.target.value);
                  const n = Number(e.target.value.replace(",", "."));
                  set({ basePrice: n > 0 ? n : 0 });
                }}
                className={inputCls}
              />
            </Field>
            <Field label={t("sec.purchases.product.currency")} htmlFor="p-cur">
              <select
                id="p-cur"
                value={p.currency}
                onChange={(e) => set({ currency: e.target.value })}
                className={inputCls}
              >
                {CURRENCIES.map((c) => (
                  <option key={c}>{c}</option>
                ))}
              </select>
            </Field>
            {sub && (
              <Field label={t("sec.purchases.product.period")} htmlFor="p-period">
                <select
                  id="p-period"
                  value={p.period ?? "year"}
                  disabled={locked}
                  onChange={(e) => set({ period: e.target.value as never })}
                  className={inputCls}
                >
                  {PERIODS.map((x) => (
                    <option key={x} value={x}>
                      {t(`sec.purchases.periods.${x}`)}
                    </option>
                  ))}
                </select>
              </Field>
            )}
          </div>
          {sub && (
            <Field
              label={t("sec.purchases.product.trial")}
              help={t("sec.purchases.product.trialHelp")}
              htmlFor="p-trial"
            >
              <input
                id="p-trial"
                inputMode="numeric"
                value={p.trialDays ?? ""}
                onChange={(e) => {
                  const n = Number(e.target.value.replace(/\D/g, ""));
                  set({ trialDays: n > 0 ? n : null });
                }}
                className={cn(inputCls, "w-32")}
                placeholder="0"
              />
            </Field>
          )}
          <Field label={t("sec.purchases.product.id")} htmlFor="p-id">
            <input
              id="p-id"
              value={p.id}
              disabled={locked}
              aria-invalid={!idOk && !!p.id}
              onChange={(e) => {
                setIdTouched(true);
                set({ id: e.target.value.trim().toLowerCase() });
              }}
              className={cn(inputCls, "font-mono")}
            />
            <p className="t-small text-muted-foreground font-mono">
              {bundleId}.{p.id || "…"}
            </p>
            <p className="t-small text-warning" role="note">
              {t(locked ? "sec.purchases.product.idLocked" : "sec.purchases.product.idWarning")}
            </p>
            {!idOk && p.id && (
              <p role="alert" className="text-destructive t-small">
                {t("sec.purchases.problems.id_invalid")}
              </p>
            )}
          </Field>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button disabled={!valid} onClick={() => onSave({ ...p, basePrice: priceNum, name: p.name.trim() })}>
            {t("sec.common.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
