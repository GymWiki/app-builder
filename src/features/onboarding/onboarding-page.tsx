import { ArrowDown, ArrowUp, ArrowLeft, Loader2, Plus, Sparkles, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Logo } from "@/components/layout/top-bar";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useLic } from "@/features/setup/license-flow";
import { api } from "@/lib/ipc";
import {
  addAssumption,
  addPage,
  movePage,
  removeAssumption,
  removePage,
  renamePage,
  setAssumption,
  setNavigation,
  setPurpose,
  setSignIn,
  setSync,
  type Blueprint,
} from "@/lib/blueprint";
import { toastError } from "@/lib/toast";
import { cn } from "@/lib/utils";

interface Feat {
  id: string;
  title: string;
  capability: string;
  status: "now" | "later" | "off";
}
interface Unsupported {
  id: string;
  title: string;
  alternative: string;
}
interface Card {
  id: string;
  seed: string;
  font: string;
  kit: string;
}
interface Draft {
  step: 0 | 1 | 2 | 3;
  description: string;
  features: Feat[] | null;
  unsupported: Unsupported[];
  cards: Card[] | null;
  styleId: string;
  seed: string | null;
  blueprint: Blueprint | null;
  fallback: boolean;
  issues: string[];
  names: string[];
}
const EMPTY: Draft = { step: 0, description: "", features: null, unsupported: [], cards: null, styleId: "", seed: null, blueprint: null, fallback: false, issues: [], names: [] };

const inputCls =
  "bg-card placeholder:text-muted-foreground focus-visible:border-ring h-10 w-full rounded-lg border px-3 t-body outline-none transition-colors disabled:opacity-60";
let uid = 0;
const fid = () => `f${Date.now().toString(36)}${uid++}`;

export default function OnboardingPage() {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const lang = i18n.language.slice(0, 2);
  const [d, setD] = useState<Draft>(EMPTY);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState<null | "features" | "plan" | "build">(null);
  const [restored, setRestored] = useState(false);
  const seq = useRef(0);
  const patch = (p: Partial<Draft>) => setD((cur) => ({ ...cur, ...p }));
  const lic = useLic();

  // Een onderbroken onboarding hervatten zonder dataverlies: het concept staat lokaal.
  useEffect(() => {
    void api
      .onboardingDraftGet()
      .then((raw) => {
        if (raw) {
          try {
            const v = JSON.parse(raw) as Draft;
            if (v.description) {
              setD({ ...EMPTY, ...v });
              setRestored(true);
            }
          } catch {
            /* een kapot concept negeren we */
          }
        }
      })
      .catch(() => undefined)
      .finally(() => setLoaded(true));
  }, []);
  useEffect(() => {
    if (!loaded) return;
    const id = setTimeout(() => void api.onboardingDraftSet(d.description ? JSON.stringify(d) : null).catch(() => undefined), 400);
    return () => clearTimeout(id);
  }, [d, loaded]);

  const startOver = () => {
    seq.current++;
    setD(EMPTY);
    setRestored(false);
    setBusy(null);
  };

  const loadFeatures = async (description: string) => {
    const my = ++seq.current;
    setBusy("features");
    try {
      const r = await api.onboardingFeatures(description, lang);
      if (my !== seq.current) return;
      patch({
        features: r.features.map((f) => ({ id: fid(), title: f.title, capability: f.capability, status: "now" as const })),
        unsupported: r.unsupported,
      });
    } catch (e) {
      toastError(e, t);
    } finally {
      if (my === seq.current) setBusy(null);
    }
  };

  const loadPlan = async (draft: Draft, cardId: string) => {
    const my = ++seq.current;
    setBusy("plan");
    try {
      const wanted = (draft.features ?? []).filter((f) => f.status !== "off").map((f) => (f.status === "later" ? `${f.title} (later)` : f.title));
      const r = await api.onboardingPlan(draft.description, wanted, cardId, draft.seed, lang);
      if (my !== seq.current) return;
      patch({ blueprint: r.blueprint, fallback: r.fallback, issues: r.issues, names: r.nameSuggestions, step: 3 });
    } catch (e) {
      toastError(e, t);
    } finally {
      if (my === seq.current) setBusy(null);
    }
  };

  const ensureCards = async (description: string) => {
    if (d.cards) return;
    const cards = await api.onboardingStyles(description);
    patch({ cards, styleId: d.styleId || cards[0]?.id || "" });
  };

  const next = async () => {
    if (d.step === 0) {
      patch({ step: 1 });
      if (!d.features) void loadFeatures(d.description);
    } else if (d.step === 1) {
      patch({ step: 2 });
      await ensureCards(d.description);
    } else if (d.step === 2) {
      patch({ step: 3 });
      await loadPlan(d, d.styleId || d.cards?.[0]?.id || "calm");
    }
  };
  const surprise = async () => {
    const cards = await api.onboardingStyles(d.description);
    const draft = { ...d, cards, styleId: cards[0]!.id, step: 3 as const };
    setD(draft);
    await loadPlan(draft, cards[0]!.id);
  };

  const build = async () => {
    if (!d.blueprint) return;
    setBusy("build");
    try {
      const p = await api.onboardingBuild(d.blueprint);
      await api.onboardingDraftSet(null).catch(() => undefined);
      navigate(`/build/${p.id}`, { replace: true });
    } catch (e) {
      setBusy(null);
      toastError(e, t);
    }
  };

  const canNext = d.step === 0 ? d.description.trim().length > 0 : d.step === 1 ? !busy : d.step === 2 ? !!d.styleId : false;
  const steps = ["idea", "features", "style", "plan"] as const;

  return (
    <div className="h-full overflow-y-auto">
      <header className="mx-auto flex max-w-3xl items-center justify-between px-6 pt-6">
        <div className="flex items-center gap-2.5">
          <Logo />
          <span className="t-heading">{t("onboarding.title")}</span>
        </div>
        <Button variant="ghost" size="icon" aria-label={t("common.cancel")} title={t("common.cancel")} onClick={() => navigate("/")}>
          <X />
        </Button>
      </header>

      <main className="mx-auto max-w-3xl px-6 pt-6 pb-24">
        <ol className="mb-8 flex items-center gap-2" aria-label={t("onboarding.progress")}>
          {steps.map((s, i) => (
            <li key={s} aria-current={d.step === i ? "step" : undefined} className="flex items-center gap-2">
              <span
                className={cn("grid size-7 place-items-center rounded-full border text-xs font-semibold", i === d.step ? "bg-primary text-primary-foreground border-primary" : i < d.step ? "bg-accent" : "text-muted-foreground")}
              >
                {i + 1}
              </span>
              <span className={cn("t-small hidden sm:inline", i === d.step ? "font-semibold" : "text-muted-foreground")}>{t(`onboarding.steps.${s}`)}</span>
              {i < steps.length - 1 && <span className="bg-border h-px w-6" aria-hidden />}
            </li>
          ))}
        </ol>

        {restored && (
          <p role="status" className="bg-accent/50 t-small mb-4 flex items-center justify-between rounded-xl border p-3">
            {t("onboarding.restored")}
            <Button size="xs" variant="ghost" onClick={startOver}>
              {t("onboarding.startOver")}
            </Button>
          </p>
        )}
        {lic.running && (
          <p role="status" className="text-muted-foreground t-small mb-4">
            {t("onboarding.toolchainBackground")}
          </p>
        )}

        {d.step === 0 && <StepIdea d={d} patch={patch} onSurprise={() => void surprise()} blank={() => navigate("/?blank=1")} />}
        {d.step === 1 && <StepFeatures d={d} patch={patch} busy={busy === "features"} />}
        {d.step === 2 && <StepStyle d={d} patch={patch} />}
        {d.step === 3 && <StepPlan d={d} patch={patch} busy={busy === "plan"} />}

        <div className="bg-background/90 fixed inset-x-0 bottom-0 border-t backdrop-blur">
          <div className="mx-auto flex max-w-3xl items-center justify-between gap-3 px-6 py-3">
            <Button variant="ghost" disabled={d.step === 0 || busy === "build"} onClick={() => patch({ step: (d.step - 1) as Draft["step"] })}>
              <ArrowLeft /> {t("common.back")}
            </Button>
            {d.step < 3 ? (
              <Button disabled={!canNext || !!busy && d.step !== 1} onClick={() => void next()}>
                {t("onboarding.next")}
              </Button>
            ) : (
              <Button disabled={!d.blueprint || busy !== null} onClick={() => void build()}>
                {busy === "build" ? <Loader2 className="animate-spin" /> : <Sparkles />} {t("onboarding.build")}
              </Button>
            )}
          </div>
        </div>
      </main>
    </div>
  );
}

// ───────────── stap 1: het idee ─────────────

function StepIdea({ d, patch, onSurprise, blank }: { d: Draft; patch: (p: Partial<Draft>) => void; onSurprise: () => void; blank: () => void }) {
  const { t } = useTranslation();
  const examples = t("onboarding.idea.examples", { returnObjects: true }) as string[];
  const quickNames = t("onboarding.idea.quickNames", { returnObjects: true }) as string[];
  const quickTexts = t("onboarding.idea.quickTexts", { returnObjects: true }) as string[];
  // Een andere beschrijving maakt een eerder voorstel ongeldig.
  const set = (description: string) => patch({ description, features: null, unsupported: [], blueprint: null, issues: [] });
  return (
    <section className="space-y-5">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="t-display">{t("onboarding.idea.title")}</h1>
          <p className="text-muted-foreground t-body mt-1">{t("onboarding.idea.help")}</p>
        </div>
        <Button variant="outline" size="sm" disabled={!d.description.trim()} onClick={onSurprise}>
          <Sparkles /> {t("onboarding.idea.surprise")}
        </Button>
      </div>
      <textarea
        autoFocus
        value={d.description}
        onChange={(e) => set(e.target.value)}
        rows={6}
        maxLength={4000}
        aria-label={t("onboarding.idea.title")}
        placeholder={t("onboarding.idea.placeholder")}
        className="bg-card focus-visible:border-ring t-body w-full resize-none rounded-xl border p-4 text-lg outline-none"
      />
      <div>
        <p className="t-small text-muted-foreground mb-2 font-medium">{t("onboarding.idea.examplesLabel")}</p>
        <div className="flex flex-wrap gap-2">
          {examples.map((e) => (
            <button key={e} type="button" onClick={() => set(e)} className="hover:bg-muted t-small rounded-full border px-3 py-1.5">
              {e}
            </button>
          ))}
        </div>
      </div>
      <div>
        <p className="t-small text-muted-foreground mb-2 font-medium">{t("onboarding.idea.quickLabel")}</p>
        <div className="flex flex-wrap gap-2">
          {quickNames.map((name, i) => (
            <Button key={name} variant="outline" size="sm" onClick={() => set(quickTexts[i] ?? name)}>
              {name}
            </Button>
          ))}
        </div>
      </div>
      <button type="button" onClick={blank} className="text-muted-foreground hover:text-foreground t-small underline underline-offset-2">
        {t("onboarding.idea.blank")}
      </button>
    </section>
  );
}

// ───────────── stap 2: de functies ─────────────

function StepFeatures({ d, patch, busy }: { d: Draft; patch: (p: Partial<Draft>) => void; busy: boolean }) {
  const { t } = useTranslation();
  const [adding, setAdding] = useState("");
  const list = d.features ?? [];
  const upd = (id: string, p: Partial<Feat>) => patch({ features: list.map((f) => (f.id === id ? { ...f, ...p } : f)) });
  const move = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    if (j < 0 || j >= list.length) return;
    const n = [...list];
    [n[i], n[j]] = [n[j]!, n[i]!];
    patch({ features: n });
  };
  return (
    <section className="space-y-5">
      <div>
        <h1 className="t-display">{t("onboarding.features.title")}</h1>
        <p className="text-muted-foreground t-body mt-1">{t("onboarding.features.help")}</p>
      </div>
      {d.unsupported.map((u) => (
        <div key={u.id} role="alert" data-testid="unsupported" className="border-warning/50 bg-warning/10 rounded-xl border p-4">
          <p className="t-body font-medium">{t("onboarding.features.unsupportedTitle", { title: u.title })}</p>
          <p className="t-small mt-1">{t("onboarding.features.unsupportedBody", { alternative: u.alternative })}</p>
          <div className="mt-2 flex gap-2">
            <Button
              size="xs"
              onClick={() => patch({ features: [...list, { id: fid(), title: u.alternative.replace(/\.$/, ""), capability: "share_text_image", status: "now" }], unsupported: d.unsupported.filter((x) => x.id !== u.id) })}
            >
              {t("onboarding.features.useAlternative")}
            </Button>
            <Button size="xs" variant="ghost" onClick={() => patch({ unsupported: d.unsupported.filter((x) => x.id !== u.id) })}>
              {t("onboarding.features.dismiss")}
            </Button>
          </div>
        </div>
      ))}
      {busy ? (
        <div role="status" className="space-y-2" aria-label={t("onboarding.features.loading")}>
          <p className="t-small text-muted-foreground inline-flex items-center gap-2">
            <Loader2 className="size-4 animate-spin" /> {t("onboarding.features.loading")}
          </p>
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="bg-muted h-12 animate-pulse rounded-xl" />
          ))}
        </div>
      ) : (
        <ul className="space-y-2" aria-label={t("onboarding.features.title")}>
          {list.map((f, i) => (
            <li key={f.id} className={cn("bg-card flex items-center gap-2 rounded-xl border p-2", f.status === "off" && "opacity-50")}>
              <Switch checked={f.status !== "off"} onCheckedChange={(v) => upd(f.id, { status: v ? "now" : "off" })} aria-label={t("onboarding.features.on", { title: f.title })} />
              <input value={f.title} onChange={(e) => upd(f.id, { title: e.target.value })} aria-label={t("onboarding.features.name")} className="t-body min-w-0 flex-1 bg-transparent px-1 py-1 outline-none" />
              <select
                value={f.status === "later" ? "later" : "now"}
                disabled={f.status === "off"}
                onChange={(e) => upd(f.id, { status: e.target.value as "now" | "later" })}
                aria-label={t("onboarding.features.when")}
                className="bg-card t-small rounded-md border px-2 py-1"
              >
                <option value="now">{t("onboarding.features.now")}</option>
                <option value="later">{t("onboarding.features.later")}</option>
              </select>
              <Button size="icon-xs" variant="ghost" aria-label={t("onboarding.moveUp")} disabled={i === 0} onClick={() => move(i, -1)}>
                <ArrowUp />
              </Button>
              <Button size="icon-xs" variant="ghost" aria-label={t("onboarding.moveDown")} disabled={i === list.length - 1} onClick={() => move(i, 1)}>
                <ArrowDown />
              </Button>
              <Button size="icon-xs" variant="ghost" aria-label={t("onboarding.remove")} onClick={() => patch({ features: list.filter((x) => x.id !== f.id) })}>
                <Trash2 />
              </Button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          const title = adding.trim();
          if (!title) return;
          patch({ features: [...list, { id: fid(), title, capability: "lists", status: "now" }] });
          setAdding("");
        }}
      >
        <input value={adding} onChange={(e) => setAdding(e.target.value)} placeholder={t("onboarding.features.addPlaceholder")} aria-label={t("onboarding.features.add")} className={inputCls} />
        <Button type="submit" variant="outline">
          <Plus /> {t("onboarding.features.add")}
        </Button>
      </form>
    </section>
  );
}

// ───────────── stap 3: de stijl ─────────────

const light = (hex: string, a: number) => `color-mix(in srgb, ${hex} ${a}%, white)`;

function MiniPreview({ seed, font, kit, name }: { seed: string; font: string; kit: string; name: string }) {
  const { t } = useTranslation();
  const r = kit === "minimal" ? "4px" : "14px";
  return (
    <div aria-hidden className="bg-white text-neutral-800 overflow-hidden rounded-xl border" style={{ fontFamily: font }}>
      <div className="px-3 py-2 text-sm font-semibold text-white" style={{ background: seed }}>
        {name}
      </div>
      <div className="space-y-2 p-3">
        <div className="h-8 w-full" style={{ background: light(seed, 14), borderRadius: r }} />
        <div className="flex gap-2">
          <div className="h-7 w-16 text-center text-xs leading-7 font-medium text-white" style={{ background: seed, borderRadius: kit === "minimal" ? "4px" : "999px" }}>
            {t("onboarding.style.sampleOk")}
          </div>
          <div className="h-7 w-16 border text-center text-xs leading-7" style={{ borderColor: seed, color: seed, borderRadius: kit === "minimal" ? "4px" : "999px" }}>
            {t("onboarding.style.sampleCancel")}
          </div>
        </div>
      </div>
    </div>
  );
}

function StepStyle({ d, patch }: { d: Draft; patch: (p: Partial<Draft>) => void }) {
  const { t } = useTranslation();
  const name = d.description.split(/\s+/).slice(0, 2).join(" ") || "App";
  return (
    <section className="space-y-5">
      <div>
        <h1 className="t-display">{t("onboarding.style.title")}</h1>
        <p className="text-muted-foreground t-body mt-1">{t("onboarding.style.help")}</p>
      </div>
      {!d.cards ? (
        <div className="grid gap-4 sm:grid-cols-3">
          {[0, 1, 2].map((i) => (
            <div key={i} className="bg-muted h-48 animate-pulse rounded-xl" />
          ))}
        </div>
      ) : (
        <div role="radiogroup" aria-label={t("onboarding.style.title")} className="grid gap-4 sm:grid-cols-3">
          {d.cards.map((c) => {
            const seed = d.styleId === c.id && d.seed ? d.seed : c.seed;
            return (
              <button
                key={c.id}
                type="button"
                role="radio"
                aria-checked={d.styleId === c.id}
                onClick={() => patch({ styleId: c.id })}
                className={cn("rounded-2xl border p-3 text-left transition-colors", d.styleId === c.id ? "border-primary ring-primary/40 ring-2" : "hover:bg-muted/50")}
              >
                <MiniPreview seed={seed} font={c.font} kit={c.kit} name={name} />
                <span className="t-body mt-3 block font-semibold">{t(`onboarding.style.cards.${c.id}`)}</span>
                <span className="t-small text-muted-foreground">{c.font} · {t(`sec.theme.kits.${c.kit}`)}</span>
              </button>
            );
          })}
        </div>
      )}
      <div className="flex flex-wrap items-center gap-3">
        <label className="t-small font-medium" htmlFor="own-color">
          {t("onboarding.style.ownColor")}
        </label>
        <input id="own-color" value={d.seed ?? ""} placeholder="#4F46E5" onChange={(e) => patch({ seed: e.target.value.trim() || null })} aria-invalid={!!d.seed && !/^#[0-9a-fA-F]{6}$/.test(d.seed)} className={cn(inputCls, "w-36 font-mono")} />
        <span className="t-small text-muted-foreground">{t("onboarding.style.optional")}</span>
      </div>
    </section>
  );
}

// ───────────── stap 4: het plan ─────────────

function StepPlan({ d, patch, busy }: { d: Draft; patch: (p: Partial<Draft>) => void; busy: boolean }) {
  const { t } = useTranslation();
  const bp = d.blueprint;
  const [newPage, setNewPage] = useState("");
  const [newAssumption, setNewAssumption] = useState("");
  if (busy || !bp) {
    return (
      <section role="status" className="space-y-4" aria-live="polite">
        <h1 className="t-display">{t("onboarding.plan.loadingTitle")}</h1>
        <p className="text-muted-foreground t-body inline-flex items-center gap-2">
          <Loader2 className="size-4 animate-spin" /> {t("onboarding.plan.loadingBody")}
        </p>
        {[0, 1, 2].map((i) => (
          <div key={i} className="bg-muted h-16 animate-pulse rounded-xl" />
        ))}
      </section>
    );
  }
  const set = (b: Blueprint) => patch({ blueprint: b });
  const wantsSync = bp.integrations.storage !== "local";
  return (
    <section className="space-y-6">
      <div>
        <h1 className="t-display">{t("onboarding.plan.title")}</h1>
        <p className="text-muted-foreground t-body mt-1">{t("onboarding.plan.help")}</p>
      </div>
      {d.fallback && (
        <p role="status" className="border-warning/50 bg-warning/10 t-small rounded-xl border p-3">
          {t("onboarding.plan.fallback")}
        </p>
      )}
      {d.issues.length > 0 && (
        <div role="alert" className="border-destructive/40 bg-destructive/8 rounded-xl border p-3">
          <p className="t-small font-medium">{t("onboarding.plan.issues")}</p>
          <ul className="t-small list-disc pl-5">
            {d.issues.slice(0, 5).map((i) => (
              <li key={i}>{i}</li>
            ))}
          </ul>
        </div>
      )}

      <div className="space-y-2">
        <label htmlFor="app-name" className="t-small font-medium">
          {t("onboarding.plan.name")}
        </label>
        <input id="app-name" value={bp.app.name} maxLength={30} onChange={(e) => set({ ...bp, app: { ...bp.app, name: e.target.value } })} className={inputCls} />
        {d.names.length > 0 && (
          <div className="flex flex-wrap gap-2">
            {d.names.map((n) => (
              <button key={n} type="button" onClick={() => set({ ...bp, app: { ...bp.app, name: n } })} className="hover:bg-muted t-small rounded-full border px-3 py-1">
                {n}
              </button>
            ))}
          </div>
        )}
      </div>

      <div>
        <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
          <h2 className="t-heading">{t("onboarding.plan.pages")}</h2>
          <div role="radiogroup" aria-label={t("onboarding.plan.navigation")} className="flex gap-1.5">
            {(["tabs", "menu", "single"] as const).map((k) => (
              <button key={k} type="button" role="radio" aria-checked={bp.navigation.kind === k} onClick={() => set(setNavigation(bp, k))} className={cn("t-small rounded-lg border px-2.5 py-1", bp.navigation.kind === k ? "border-primary bg-primary/10 font-medium" : "hover:bg-muted")}>
                {t(`onboarding.plan.nav.${k}`)}
              </button>
            ))}
          </div>
        </div>
        <ul className="space-y-2" aria-label={t("onboarding.plan.pages")}>
          {bp.pages.map((p, i) => (
            <li key={p.id} className="bg-card rounded-xl border p-3">
              <div className="flex items-center gap-2">
                <input value={p.name} maxLength={30} onChange={(e) => set(renamePage(bp, p.id, e.target.value))} aria-label={t("onboarding.plan.pageName")} className="t-body min-w-0 flex-1 bg-transparent font-medium outline-none" />
                <span className="t-small text-muted-foreground font-mono">{p.route}</span>
                <Button size="icon-xs" variant="ghost" aria-label={t("onboarding.moveUp")} disabled={i === 0} onClick={() => set(movePage(bp, p.id, -1))}>
                  <ArrowUp />
                </Button>
                <Button size="icon-xs" variant="ghost" aria-label={t("onboarding.moveDown")} disabled={i === bp.pages.length - 1} onClick={() => set(movePage(bp, p.id, 1))}>
                  <ArrowDown />
                </Button>
                <Button size="icon-xs" variant="ghost" aria-label={t("onboarding.plan.removePage", { name: p.name })} disabled={p.route === "/" || bp.pages.length <= 1} onClick={() => set(removePage(bp, p.id))}>
                  <Trash2 />
                </Button>
              </div>
              <input value={p.purpose} onChange={(e) => set(setPurpose(bp, p.id, e.target.value))} placeholder={t("onboarding.plan.purposePlaceholder")} aria-label={t("onboarding.plan.purpose")} className="t-small text-muted-foreground mt-1 w-full bg-transparent outline-none" />
              {p.components.length > 0 && <p className="t-small text-muted-foreground mt-1">{p.components.join(" · ")}</p>}
            </li>
          ))}
        </ul>
        <form
          className="mt-2 flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            set(addPage(bp, newPage));
            setNewPage("");
          }}
        >
          <input value={newPage} onChange={(e) => setNewPage(e.target.value)} placeholder={t("onboarding.plan.addPagePlaceholder")} aria-label={t("onboarding.plan.addPage")} className={inputCls} />
          <Button type="submit" variant="outline">
            <Plus /> {t("onboarding.plan.addPage")}
          </Button>
        </form>
      </div>

      <div>
        <h2 className="t-heading mb-1">{t("onboarding.plan.data")}</h2>
        <p className="t-small text-muted-foreground">
          {bp.models.length ? t("onboarding.plan.dataList", { models: bp.models.map((m) => `${m.name} (${m.fields.length})`).join(", ") }) : t("onboarding.plan.dataNone")}
        </p>
      </div>

      <div>
        <h2 className="t-heading mb-1">{t("onboarding.plan.connections")}</h2>
        <p className="t-small text-muted-foreground mb-2">{t("onboarding.plan.connectionsDefault")}</p>
        <div className="space-y-2">
          <label className="bg-card flex items-start justify-between gap-3 rounded-xl border p-3">
            <span>
              <span className="t-body block font-medium">{t("onboarding.plan.signIn")}</span>
              <span className="t-small text-muted-foreground">{t("onboarding.plan.signInHelp")}</span>
            </span>
            <Switch checked={bp.integrations.auth.length > 0} onCheckedChange={(v) => set(setSignIn(bp, v))} aria-label={t("onboarding.plan.signIn")} />
          </label>
          <label className="bg-card flex items-start justify-between gap-3 rounded-xl border p-3">
            <span>
              <span className="t-body block font-medium">{t("onboarding.plan.sync")}</span>
              <span className="t-small text-muted-foreground">{t("onboarding.plan.syncHelp")}</span>
            </span>
            <Switch checked={wantsSync} onCheckedChange={(v) => set(setSync(bp, v ? "supabase" : null))} aria-label={t("onboarding.plan.sync")} />
          </label>
        </div>
      </div>

      <div>
        <h2 className="t-heading mb-1">{t("onboarding.plan.assumptions")}</h2>
        <ul className="space-y-1.5" aria-label={t("onboarding.plan.assumptions")}>
          {bp.assumptions.map((a) => (
            <li key={a.id} className="flex items-center gap-2">
              <input value={a.text} onChange={(e) => set(setAssumption(bp, a.id, e.target.value))} aria-label={t("onboarding.plan.assumption")} className={inputCls} />
              <Button size="icon-xs" variant="ghost" aria-label={t("onboarding.remove")} onClick={() => set(removeAssumption(bp, a.id))}>
                <Trash2 />
              </Button>
            </li>
          ))}
        </ul>
        <form
          className="mt-2 flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            set(addAssumption(bp, newAssumption));
            setNewAssumption("");
          }}
        >
          <input value={newAssumption} onChange={(e) => setNewAssumption(e.target.value)} placeholder={t("onboarding.plan.addAssumptionPlaceholder")} aria-label={t("onboarding.plan.addAssumption")} className={inputCls} />
          <Button type="submit" variant="outline">
            <Plus /> {t("onboarding.plan.addAssumption")}
          </Button>
        </form>
      </div>
    </section>
  );
}
