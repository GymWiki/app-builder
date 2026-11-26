import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, CheckCircle2, Loader2, Pencil, Plus, ShoppingBag, Sparkles, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { StatusBadge, type Tone } from "@/components/ds/status-badge";
import { Card2, Check, Field, SectionPage, inputCls } from "@/features/sections/shared";
import { useSettings } from "@/hooks";
import { friendlyError } from "@/lib/errors";
import { api, onEvent } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { PaywallSettings, Project, PurchaseProduct, Purchases, PurchasesView, StoreStatus } from "@/lib/types";
import { cn } from "@/lib/utils";
import { askClaude, jsonFrom, paywallTweakPrompt } from "./ask";
import { emptyProduct, isLocked, money, unlocksOf } from "./format";
import { PaywallPreview } from "./paywall-preview";
import { ProductDialog } from "./product-dialog";

const TONE: Record<StoreStatus, Tone> = {
  concept: "neutral",
  inApp: "info",
  created: "accent",
  inReview: "warning",
  live: "success",
};

/** De pagina "Aankopen": producten, wat achter de betaalmuur zit, de betaalmuur zelf, en "Add to app" / "Update app". */
export function PurchasesPage({ project, goto }: { project: Project; goto: (dest: string) => void }) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const id = project.id;
  const dev = useSettings().data?.dev_mode === "true";
  const q = useQuery({ queryKey: ["purchases", id], queryFn: () => api.purchasesGet(id) });
  const theme = useQuery({ queryKey: ["theme", id], queryFn: () => api.themeGet(id) });
  const view = q.data;
  const [editing, setEditing] = useState<PurchaseProduct | null>(null);
  const [removing, setRemoving] = useState<PurchaseProduct | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ step: string; detail?: string | null } | null>(null);
  const [desc, setDesc] = useState<string | null>(null);
  const cancel = useRef({ cancelled: false });

  const setView = (v: PurchasesView) => qc.setQueryData(["purchases", id], v);
  const refresh = () => void qc.invalidateQueries({ queryKey: ["purchases", id] });

  useEffect(() => {
    const un = onEvent<{ projectId: string; step: string; detail?: string | null }>("purchases-progress", (e) => {
      if (e.projectId !== id) return;
      setProgress(e);
      if (e.step === "done" || e.step === "failed") {
        refresh();
        void qc.invalidateQueries({ queryKey: ["checklist", id] });
      }
    });
    return () => void un.then((f) => f());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);
  // (In de ontwikkelweergave draait een effect twee keer: daarom bij het starten weer op "niet afgebroken" zetten.)
  useEffect(() => {
    const c = cancel.current;
    c.cancelled = false;
    return () => void (c.cancelled = true);
  }, []);

  if (!view) {
    return (
      <SectionPage title={t("sec.purchases.title")} intro={t("sec.purchases.intro")}>
        <p role="status" className="text-muted-foreground t-body">
          {t("common.loading")}
        </p>
      </SectionPage>
    );
  }
  const p = view.purchases;
  const locale = i18n.language;
  const seed = theme.data?.seedColor ?? "#4F46E5";

  /** Eén wijziging opslaan; de backend geeft de nieuwe staat terug. */
  const commit = async (mutate: (next: Purchases) => void) => {
    // Altijd van de nieuwste staat uitgaan (een trage vraag aan Claude mag eerdere wijzigingen niet overschrijven).
    const next = structuredClone(qc.getQueryData<PurchasesView>(["purchases", id])?.purchases ?? p);
    mutate(next);
    try {
      setView(await api.purchasesSave(id, next));
    } catch (e) {
      toastError(e, t);
    }
  };

  const saveProduct = (pr: PurchaseProduct) => {
    const isNew = !p.products.some((x) => x.id === (editing?.id ?? "") && editing?.id);
    void commit((n) => {
      const i = n.products.findIndex((x) => x.id === editing?.id);
      if (i >= 0) n.products[i] = pr;
      else n.products.push(pr);
      // Een nieuw product komt vanzelf in de betaalmuur; het eerste abonnement of de eerste aankoop is uitgelicht.
      if (isNew) {
        n.paywall.productIds = [...n.paywall.productIds.filter((x) => x !== pr.id), pr.id];
        if (!n.paywall.featured) n.paywall.featured = pr.id;
        n.provider = n.products.some((x) => x.kind === "subscription") ? "revenueCat" : "inAppPurchase";
      }
    });
    setEditing(null);
  };

  const removeProduct = (pr: PurchaseProduct) =>
    void commit((n) => {
      n.products = n.products.filter((x) => x.id !== pr.id);
      n.paywall.productIds = n.paywall.productIds.filter((x) => x !== pr.id);
      if (n.paywall.featured === pr.id) n.paywall.featured = n.paywall.productIds[0] ?? null;
    });

  const makeGatingPlan = async () => {
    setBusy("gating");
    try {
      const prompt = await api.purchasesGatingPrompt(id);
      const answer = await askClaude(id, prompt, i18n.language.slice(0, 2), cancel.current);
      setView(await api.purchasesSetGatingFromAnswer(id, answer));
    } catch (e) {
      toast.error(friendlyError(e, t).title);
    } finally {
      setBusy(null);
    }
  };

  const tweakPaywall = async (request: string) => {
    setBusy("tweak");
    try {
      const answer = await askClaude(
        id,
        paywallTweakPrompt(p.paywall, request),
        i18n.language.slice(0, 2),
        cancel.current,
      );
      const next = jsonFrom(answer) as Partial<PaywallSettings>;
      await commit((n) => {
        n.paywall = { ...n.paywall, ...next, productIds: n.paywall.productIds, featured: n.paywall.featured };
      });
    } catch (e) {
      toast.error(friendlyError(e, t).title);
    } finally {
      setBusy(null);
    }
  };

  const apply = async () => {
    setConfirm(false);
    setProgress({ step: "snapshot" });
    try {
      await api.purchasesApply(id, i18n.language.slice(0, 2), true);
      refresh();
    } catch (e) {
      setProgress(null);
      toastError(e, t);
    }
  };

  const working = view.applying || (progress !== null && progress.step !== "done" && progress.step !== "failed");
  const first = view.plan.firstTime;
  const nothing =
    !first &&
    view.plan.create.length +
      view.plan.update.length +
      view.plan.remove.length +
      view.plan.gatePointsAdded +
      view.plan.gatePointsRemoved ===
      0 &&
    !view.plan.paywallChanged &&
    !view.plan.providerChanged;
  const blocked = view.missing.length > 0 || nothing || working;

  return (
    <SectionPage title={t("sec.purchases.title")} intro={t("sec.purchases.intro")}>
      {/* 1. producten */}
      <Card2 title={t("sec.purchases.products.title")}>
        <p className="t-small text-muted-foreground">{t("sec.purchases.products.help")}</p>
        {p.products.length === 0 ? (
          <div className="bg-muted/50 rounded-xl border border-dashed p-5 text-center">
            <ShoppingBag aria-hidden className="text-muted-foreground mx-auto mb-2 size-5" />
            <p className="t-body font-medium">{t("sec.purchases.products.none")}</p>
            <p className="t-small text-muted-foreground mt-1">{t("sec.purchases.products.noneHint")}</p>
          </div>
        ) : (
          <ul className="space-y-2" aria-label={t("sec.purchases.products.title")}>
            {p.products.map((pr) => (
              <li key={pr.id} className="rounded-xl border p-3">
                <div className="flex flex-wrap items-start gap-2">
                  <div className="min-w-0 flex-1">
                    <p className="t-body font-medium">{pr.name}</p>
                    <p className="t-small text-muted-foreground">
                      {t(`sec.purchases.kinds.${pr.kind}`)} · {money(pr.basePrice, pr.currency, locale)}
                      {pr.period ? ` / ${t(`sec.purchases.periods.${pr.period}`)}` : ""}
                      {pr.trialDays ? ` · ${t("sec.purchases.paywall.trial", { days: pr.trialDays })}` : ""}
                      {pr.entitlement ? ` · ${t("sec.purchases.products.unlocks", { name: pr.entitlement })}` : ""}
                    </p>
                    <p className="t-small text-muted-foreground font-mono">
                      {view.bundleId}.{pr.id}
                    </p>
                  </div>
                  <div className="flex items-center gap-1">
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      aria-label={t("sec.purchases.products.edit", { name: pr.name })}
                      onClick={() => setEditing(pr)}
                    >
                      <Pencil />
                    </Button>
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      aria-label={t("sec.purchases.products.remove", { name: pr.name })}
                      onClick={() => (isLocked(pr) ? setRemoving(pr) : removeProduct(pr))}
                    >
                      <Trash2 />
                    </Button>
                  </div>
                </div>
                <div className="mt-2 flex flex-wrap gap-2">
                  {(["apple", "google"] as const)
                    .filter((s) => p[s])
                    .map((s) => (
                      <StatusBadge key={s} tone={TONE[pr.status[s]]}>
                        {t(`sec.purchases.stores.${s}`)}: {t(`sec.purchases.status.${pr.status[s]}`)}
                      </StatusBadge>
                    ))}
                </div>
              </li>
            ))}
          </ul>
        )}
        <div className="flex flex-wrap items-center gap-3">
          <Button variant="outline" onClick={() => setEditing(emptyProduct())}>
            <Plus /> {t("sec.purchases.products.add")}
          </Button>
          <div className="ml-auto flex flex-wrap items-center gap-4">
            <Check
              id="store-apple"
              checked={p.apple}
              onChange={(v) => void commit((n) => void (n.apple = v))}
              label={t("sec.purchases.stores.apple")}
            />
            <Check
              id="store-google"
              checked={p.google}
              onChange={(v) => void commit((n) => void (n.google = v))}
              label={t("sec.purchases.stores.google")}
            />
          </div>
        </div>
        {p.products.length > 0 && (
          <Field
            label={t("sec.purchases.provider.label")}
            help={t(
              p.provider === "revenueCat" ? "sec.purchases.provider.revenueCatWhy" : "sec.purchases.provider.iapWhy",
            )}
            htmlFor="provider"
          >
            <select
              id="provider"
              value={p.provider}
              onChange={(e) => void commit((n) => void (n.provider = e.target.value as never))}
              className={cn(inputCls, "sm:w-72")}
            >
              <option value="revenueCat">{t("sec.purchases.provider.revenueCat")}</option>
              <option value="inAppPurchase">{t("sec.purchases.provider.iap")}</option>
            </select>
          </Field>
        )}
      </Card2>

      {/* 2. achter de betaalmuur */}
      <Card2 title={t("sec.purchases.gating.title")}>
        <Field
          label={t("sec.purchases.gating.describe")}
          help={t("sec.purchases.gating.describeHelp")}
          htmlFor="gate-desc"
        >
          <textarea
            id="gate-desc"
            rows={3}
            value={desc ?? p.paywallDescription}
            onChange={(e) => setDesc(e.target.value)}
            onBlur={() =>
              desc !== null &&
              desc !== p.paywallDescription &&
              void commit((n) => void (n.paywallDescription = desc)).then(() => setDesc(null))
            }
            className={cn(inputCls, "h-auto py-2")}
            placeholder={t("sec.purchases.gating.placeholder")}
          />
        </Field>
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="outline"
            disabled={busy !== null || !(desc ?? p.paywallDescription).trim()}
            onClick={() => void makeGatingPlan()}
          >
            {busy === "gating" ? <Loader2 className="animate-spin" /> : <Sparkles />} {t("sec.purchases.gating.make")}
          </Button>
          {busy === "gating" && (
            <span role="status" className="t-small text-muted-foreground">
              {t("sec.purchases.gating.working")}
            </span>
          )}
        </div>
        {p.gating.points.length > 0 && (
          <div className="bg-muted/40 rounded-xl border p-3" data-testid="gating-plan">
            <p className="t-body font-medium">{t("sec.purchases.gating.planTitle")}</p>
            <ul className="mt-1 space-y-1">
              {p.gating.points.map((g, i) => (
                <li key={`${g.name}${i}`} className="t-body flex items-center gap-2">
                  <span className="flex-1">
                    {t("sec.purchases.gating.item", {
                      kind: t(`sec.purchases.gating.kinds.${g.kind}`),
                      name: g.name,
                      unlock: g.entitlement,
                    })}
                    {dev && g.file ? (
                      <span className="text-muted-foreground t-small font-mono">
                        {" "}
                        · {g.file}
                        {g.line ? `:${g.line}` : ""}
                      </span>
                    ) : null}
                  </span>
                  <Button
                    size="icon-xs"
                    variant="ghost"
                    aria-label={t("sec.purchases.gating.removeItem", { name: g.name })}
                    onClick={() => void commit((n) => void n.gating.points.splice(i, 1))}
                  >
                    <X />
                  </Button>
                </li>
              ))}
            </ul>
            <div className="mt-3 flex flex-wrap items-center gap-2">
              {p.gating.approved ? (
                <StatusBadge tone="success">
                  <CheckCircle2 className="size-3.5" /> {t("sec.purchases.gating.approved")}
                </StatusBadge>
              ) : (
                <Button
                  size="sm"
                  onClick={() =>
                    void api
                      .purchasesApproveGating(id)
                      .then(setView)
                      .catch((e) => toastError(e, t))
                  }
                >
                  {t("sec.purchases.gating.approve")}
                </Button>
              )}
              <Button size="sm" variant="outline" disabled={busy !== null} onClick={() => void makeGatingPlan()}>
                {t("sec.purchases.gating.again")}
              </Button>
              <span className="t-small text-muted-foreground">{t("sec.purchases.gating.adjustHint")}</span>
            </div>
          </div>
        )}
      </Card2>

      {/* 3. betaalmuur + 4. opties */}
      <Card2 title={t("sec.purchases.paywall.title")}>
        <div className="grid gap-5 lg:grid-cols-[1fr_auto]">
          <div className="space-y-4">
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label={t("sec.purchases.paywall.where")} htmlFor="pw-where">
                <select
                  id="pw-where"
                  value={p.paywall.placement}
                  onChange={(e) => void commit((n) => void (n.paywall.placement = e.target.value as never))}
                  className={inputCls}
                >
                  {(["lockedFeature", "startup", "settings", "afterUses"] as const).map((x) => (
                    <option key={x} value={x}>
                      {t(`sec.purchases.paywall.placements.${x}`)}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label={t("sec.purchases.paywall.layout")} htmlFor="pw-layout">
                <select
                  id="pw-layout"
                  value={p.paywall.layout}
                  onChange={(e) => void commit((n) => void (n.paywall.layout = e.target.value as never))}
                  className={inputCls}
                >
                  {(["fullScreen", "sheet", "compact"] as const).map((x) => (
                    <option key={x} value={x}>
                      {t(`sec.purchases.paywall.layouts.${x}`)}
                    </option>
                  ))}
                </select>
              </Field>
            </div>
            {p.paywall.placement === "afterUses" && (
              <Field label={t("sec.purchases.paywall.afterUses")} htmlFor="pw-uses">
                <input
                  id="pw-uses"
                  inputMode="numeric"
                  defaultValue={p.paywall.afterUses || 3}
                  onBlur={(e) =>
                    void commit(
                      (n) => void (n.paywall.afterUses = Math.max(1, Number(e.target.value.replace(/\D/g, "")) || 3)),
                    )
                  }
                  className={cn(inputCls, "w-28")}
                />
              </Field>
            )}
            {(["headline", "body", "cta"] as const).map((k) => (
              <Field key={k} label={t(`sec.purchases.paywall.${k}`)} htmlFor={`pw-${k}`}>
                <input
                  id={`pw-${k}`}
                  key={p.paywall[k]}
                  defaultValue={p.paywall[k]}
                  onBlur={(e) =>
                    e.target.value !== p.paywall[k] && void commit((n) => void (n.paywall[k] = e.target.value))
                  }
                  className={inputCls}
                  placeholder={t(`sec.purchases.paywall.${k}Example`)}
                />
              </Field>
            ))}
            <Field
              label={t("sec.purchases.paywall.style")}
              help={t("sec.purchases.paywall.styleHelp")}
              htmlFor="pw-style"
            >
              <input
                id="pw-style"
                key={p.paywall.style}
                defaultValue={p.paywall.style}
                onBlur={(e) =>
                  e.target.value !== p.paywall.style && void commit((n) => void (n.paywall.style = e.target.value))
                }
                className={inputCls}
                placeholder={t("sec.purchases.paywall.styleExample")}
              />
            </Field>
            {p.products.length > 0 && (
              <Field label={t("sec.purchases.paywall.products")}>
                <div className="space-y-1.5">
                  {p.products.map((pr) => (
                    <div key={pr.id} className="flex flex-wrap items-center gap-3">
                      <Check
                        id={`pw-p-${pr.id}`}
                        checked={p.paywall.productIds.includes(pr.id)}
                        onChange={(v) =>
                          void commit((n) => {
                            n.paywall.productIds = v
                              ? [...n.paywall.productIds, pr.id]
                              : n.paywall.productIds.filter((x) => x !== pr.id);
                            if (!v && n.paywall.featured === pr.id)
                              n.paywall.featured = n.paywall.productIds[0] ?? null;
                          })
                        }
                        label={pr.name}
                      />
                      <label className="t-small text-muted-foreground flex items-center gap-1.5">
                        <input
                          type="radio"
                          name="featured"
                          aria-label={t("sec.purchases.paywall.makeFeatured", { name: pr.name })}
                          checked={p.paywall.featured === pr.id}
                          disabled={!p.paywall.productIds.includes(pr.id)}
                          onChange={() => void commit((n) => void (n.paywall.featured = pr.id))}
                          className="accent-primary"
                        />
                        {t("sec.purchases.paywall.featured")}
                      </label>
                    </div>
                  ))}
                </div>
              </Field>
            )}
            <TweakBox busy={busy === "tweak"} onAsk={(r) => void tweakPaywall(r)} />
          </div>
          <div className="space-y-2">
            <PaywallPreview
              paywall={p.paywall}
              products={p.products}
              points={p.gating.points}
              seed={seed}
              dark={false}
              locale={locale}
            />
            <p className="t-small text-muted-foreground max-w-[18rem]">{t("sec.purchases.paywall.realPriceNote")}</p>
          </div>
        </div>
        {p.products.length > 0 && (
          <div className="mt-2 overflow-x-auto" data-testid="options-table">
            <p className="t-body mb-1 font-medium">{t("sec.purchases.options.title")}</p>
            <table className="t-small w-full text-left">
              <thead className="text-muted-foreground">
                <tr>
                  <th className="py-1 pr-3 font-medium">{t("sec.purchases.product.name")}</th>
                  <th className="pr-3 font-medium">{t("sec.purchases.options.base")}</th>
                  <th className="font-medium">{t("sec.purchases.options.unlocks")}</th>
                </tr>
              </thead>
              <tbody>
                {(p.paywall.productIds.length ? p.paywall.productIds : p.products.map((x) => x.id))
                  .map((pid) => p.products.find((x) => x.id === pid))
                  .filter(Boolean)
                  .map((pr) => (
                    <tr key={pr!.id} className="border-t">
                      <td className="py-1.5 pr-3">
                        {pr!.name}
                        {p.paywall.featured === pr!.id ? ` ★` : ""}
                      </td>
                      <td className="pr-3">
                        {pr!.period
                          ? t(`sec.purchases.periods.${pr!.period}`)
                          : money(pr!.basePrice, pr!.currency, locale)}
                      </td>
                      <td>
                        {unlocksOf(pr!, p.gating.points)
                          .map((g) => g.name)
                          .join(", ") ||
                          pr!.entitlement ||
                          "—"}
                      </td>
                    </tr>
                  ))}
              </tbody>
            </table>
            <p className="t-small text-muted-foreground mt-1">{t("sec.purchases.options.note")}</p>
          </div>
        )}
      </Card2>

      {/* 5/6. toepassen */}
      <Card2 title={t(first ? "sec.purchases.apply.titleFirst" : "sec.purchases.apply.titleUpdate")}>
        {view.summary.length > 0 && (
          <p className="t-body" data-testid="apply-summary">
            {t("sec.purchases.apply.will")}{" "}
            {view.summary.map(([k, n]) => t(`sec.purchases.apply.parts.${k}`, { count: n })).join(", ")}.
          </p>
        )}
        {view.plan.risks.length > 0 && (
          <ul className="space-y-1" aria-label={t("sec.purchases.apply.risksTitle")}>
            {view.plan.risks.map((r) => (
              <li key={r.code + r.product} className="t-small text-warning flex items-start gap-2">
                <AlertTriangle aria-hidden className="mt-0.5 size-4 shrink-0" />
                {t(`sec.purchases.apply.risks.${r.code}`, {
                  product: p.products.find((x) => x.id === r.product)?.name ?? r.product,
                })}
              </li>
            ))}
          </ul>
        )}
        {view.missing.length > 0 && (
          <ul className="space-y-1" aria-label={t("sec.purchases.apply.missingTitle")}>
            {view.missing.map((m) => (
              <li key={m.code} className="t-small flex flex-wrap items-center gap-2">
                <span className="text-muted-foreground">{t(`sec.purchases.missing.${m.code}`)}</span>
                {!m.next.startsWith("purchases:") && (
                  <Button size="xs" variant="outline" onClick={() => goto(m.next)}>
                    {t("sec.purchases.missing.go")}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
        <div className="flex flex-wrap items-center gap-3">
          <Button disabled={blocked} onClick={() => setConfirm(true)}>
            {working ? <Loader2 className="animate-spin" /> : <Sparkles />}{" "}
            {t(first ? "sec.purchases.apply.addToApp" : "sec.purchases.apply.updateApp")}
          </Button>
          {nothing && !working && (
            <span className="t-small text-muted-foreground">{t("sec.purchases.apply.nothing")}</span>
          )}
          {working && (
            <span role="status" className="t-small text-muted-foreground">
              {t(`sec.purchases.apply.steps.${progress?.step === "code" ? "code" : "snapshot"}`)}
            </span>
          )}
          {progress?.step === "done" && (
            <StatusBadge tone="success">
              <CheckCircle2 className="size-3.5" /> {t("sec.purchases.apply.done")}
            </StatusBadge>
          )}
          {progress?.step === "failed" && (
            <span role="alert" className="t-small text-destructive">
              {t("sec.purchases.apply.failed")}
            </span>
          )}
        </div>
      </Card2>

      {/* begeleide stappen in de winkels */}
      {p.appliedVersion > 0 && (
        <Card2 title={t("sec.purchases.stores.title")}>
          <p className="t-small text-muted-foreground">{t("sec.purchases.stores.intro")}</p>
          {view.tasks.length === 0 ? (
            <p className="t-body flex items-center gap-2">
              <CheckCircle2 aria-hidden className="text-success size-4" /> {t("sec.purchases.stores.allCreated")}
            </p>
          ) : (
            <ul className="space-y-2" data-testid="store-tasks">
              {view.tasks.map((s) => (
                <li key={s.store + s.product} className="flex flex-wrap items-center gap-3 rounded-xl border p-3">
                  <div className="min-w-0 flex-1">
                    <p className="t-body font-medium">
                      {t(`sec.purchases.stores.${s.store}`)}: {s.name}
                    </p>
                    <p className="t-small text-muted-foreground font-mono">{s.fullId}</p>
                    <p className="t-small text-muted-foreground">
                      {t(`sec.purchases.kinds.${s.kind}`)} · {money(s.basePrice, s.currency, locale)}
                      {s.period ? ` / ${t(`sec.purchases.periods.${s.period}`)}` : ""}
                      {s.trialDays ? ` · ${t("sec.purchases.paywall.trial", { days: s.trialDays })}` : ""}
                    </p>
                  </div>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() =>
                      void api
                        .purchasesSetStatus(id, s.product, s.store, "created")
                        .then(setView)
                        .catch((e) => toastError(e, t))
                    }
                  >
                    {t("sec.purchases.stores.created")}
                  </Button>
                </li>
              ))}
            </ul>
          )}
          <div className="space-y-2">
            {(
              [
                ["paid_apps_agreement", p.apple],
                ["tax_bank", p.apple],
                ["merchant_account", p.google],
                ["sandbox_tested", true],
                ["apple_submitted", p.apple],
              ] as const
            )
              .filter(([, on]) => on)
              .map(([k]) => (
                <Check
                  key={k}
                  id={`conf-${k}`}
                  checked={!!p.confirmations[k]}
                  onChange={(v) =>
                    void api
                      .purchasesConfirm(id, k, v)
                      .then(setView)
                      .catch((e) => toastError(e, t))
                  }
                  label={t(`sec.purchases.confirm.${k}`)}
                  help={t(`sec.purchases.confirm.${k}Help`)}
                />
              ))}
          </div>
          <div className="space-y-1.5">
            {p.products
              .filter((x) => x.status.apple !== "concept" || x.status.google !== "concept")
              .map((pr) => (
                <div key={pr.id} className="flex flex-wrap items-center gap-2">
                  <span className="t-small w-40 truncate font-medium">{pr.name}</span>
                  {(["apple", "google"] as const)
                    .filter((s) => p[s] && pr.status[s] !== "concept")
                    .map((s) => (
                      <label key={s} className="t-small text-muted-foreground flex items-center gap-1.5">
                        {t(`sec.purchases.stores.${s}`)}
                        <select
                          aria-label={`${t(`sec.purchases.stores.${s}`)} ${pr.name}`}
                          value={pr.status[s]}
                          onChange={(e) =>
                            void api
                              .purchasesSetStatus(id, pr.id, s, e.target.value as StoreStatus)
                              .then(setView)
                              .catch((er) => toastError(er, t))
                          }
                          className="bg-card h-8 rounded-md border px-2"
                        >
                          {(["inApp", "created", "inReview", "live"] as const).map((st) => (
                            <option key={st} value={st}>
                              {t(`sec.purchases.status.${st}`)}
                            </option>
                          ))}
                        </select>
                      </label>
                    ))}
                </div>
              ))}
          </div>
        </Card2>
      )}

      <ProductDialog
        key={editing ? `edit-${editing.id}` : "closed"}
        open={editing !== null}
        initial={editing ?? emptyProduct()}
        bundleId={view.bundleId}
        taken={p.products.map((x) => x.id)}
        onClose={() => setEditing(null)}
        onSave={saveProduct}
      />

      <AlertDialog open={confirm} onOpenChange={setConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t(first ? "sec.purchases.confirm.titleFirst" : "sec.purchases.confirm.titleUpdate")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("sec.purchases.apply.will")}{" "}
              {view.summary.map(([k, n]) => t(`sec.purchases.apply.parts.${k}`, { count: n })).join(", ")}.
            </AlertDialogDescription>
          </AlertDialogHeader>
          {view.plan.create.length > 0 && (
            <div className="t-small space-y-1">
              <p className="text-warning font-medium">{t("sec.purchases.confirm.permanent")}</p>
              <ul className="font-mono">
                {view.plan.create.map((pid) => (
                  <li key={pid}>
                    {view.bundleId}.{pid}
                  </li>
                ))}
              </ul>
              <p className="text-muted-foreground">{t("sec.purchases.confirm.deleteLimits")}</p>
            </div>
          )}
          {view.plan.risks.length > 0 && (
            <ul className="t-small text-warning space-y-1">
              {view.plan.risks.map((r) => (
                <li key={r.code + r.product}>
                  {t(`sec.purchases.apply.risks.${r.code}`, {
                    product: p.products.find((x) => x.id === r.product)?.name ?? r.product,
                  })}
                </li>
              ))}
            </ul>
          )}
          <p className="t-small text-muted-foreground">{t("sec.purchases.confirm.snapshotNote")}</p>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void apply()}>
              {t(first ? "sec.purchases.apply.addToApp" : "sec.purchases.apply.updateApp")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={removing !== null} onOpenChange={(o) => !o && setRemoving(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("sec.purchases.remove.title", { name: removing?.name })}</AlertDialogTitle>
            <AlertDialogDescription>{t("sec.purchases.remove.body")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (removing) removeProduct(removing);
                setRemoving(null);
              }}
            >
              {t("sec.purchases.remove.confirm")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </SectionPage>
  );
}

function TweakBox({ busy, onAsk }: { busy: boolean; onAsk: (request: string) => void }) {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  return (
    <Field label={t("sec.purchases.paywall.tweak")} help={t("sec.purchases.paywall.tweakHelp")} htmlFor="pw-tweak">
      <div className="flex gap-2">
        <input
          id="pw-tweak"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && text.trim() && !busy && (onAsk(text.trim()), setText(""))}
          className={inputCls}
          placeholder={t("sec.purchases.paywall.tweakExample")}
        />
        <Button
          variant="outline"
          disabled={busy || !text.trim()}
          onClick={() => {
            onAsk(text.trim());
            setText("");
          }}
        >
          {busy ? <Loader2 className="animate-spin" /> : <Sparkles />} {t("sec.purchases.paywall.tweakButton")}
        </Button>
      </div>
    </Field>
  );
}
