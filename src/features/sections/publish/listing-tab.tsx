import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Camera, ImagePlus, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { usePreview } from "@/features/preview/preview-store";
import { api, inTauri } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Meta, Questionnaire } from "@/lib/types";
import { AskButton, Card2, Check, Field, inputCls } from "../shared";

const CATS = [
  "tools",
  "productivity",
  "lifestyle",
  "health",
  "education",
  "entertainment",
  "games",
  "finance",
  "social",
  "business",
];
const QS = [
  "hasAccounts",
  "collectsPersonalData",
  "showsAds",
  "inAppPurchases",
  "userGeneratedContent",
  "usesLocation",
] as const;

export function summaryKeys(q: Questionnaire): string[] {
  const out: string[] = [];
  if (q.collectsPersonalData) out.push("privacy");
  if (q.showsAds) out.push("ads");
  if (q.inAppPurchases) out.push("purchases");
  if (q.userGeneratedContent) out.push("ugc");
  if (q.usesLocation) out.push("location");
  if (q.hasAccounts) out.push("accounts");
  return out.length ? out : ["none"];
}

function Thumb({ projectId, name, onDelete }: { projectId: string; name: string; onDelete: () => void }) {
  const { t } = useTranslation();
  const q = useQuery({
    queryKey: ["shot", projectId, "store", name],
    queryFn: () => api.fileDataUrl(projectId, `store/screenshots/${name}`).catch(() => null),
    staleTime: Infinity,
  });
  return (
    <li className="relative">
      {q.data ? (
        <img src={q.data} alt={name} className="h-44 w-auto rounded-lg border bg-white object-contain" />
      ) : (
        <div className="bg-muted h-44 w-24 rounded-lg" />
      )}
      <Button
        size="icon-xs"
        variant="secondary"
        className="absolute top-1.5 right-1.5"
        aria-label={`${t("sec.publish.listing.remove")} ${name}`}
        onClick={onDelete}
      >
        <Trash2 />
      </Button>
    </li>
  );
}

export function ListingTab({
  projectId,
  meta,
  onMeta,
}: {
  projectId: string;
  meta: Meta;
  onMeta: (m: Meta) => Promise<void>;
}) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [draft, setDraft] = useState(meta);
  const [busy, setBusy] = useState(false);
  const preview = usePreview((s) => s.state);
  const shots = useQuery({ queryKey: ["storeShots", projectId], queryFn: () => api.storeScreenshotsList(projectId) });
  const previewUp =
    preview.projectId === projectId &&
    (preview.status === "running" || preview.status === "reloading") &&
    preview.target === "web";
  const l = draft.listing;
  const setL = (p: Partial<Meta["listing"]>) => setDraft({ ...draft, listing: { ...l, ...p } });
  const setQ = (k: (typeof QS)[number], v: boolean) =>
    setDraft({ ...draft, questionnaire: { ...draft.questionnaire, [k]: v } });
  const refreshShots = () =>
    Promise.all([
      qc.invalidateQueries({ queryKey: ["storeShots", projectId] }),
      qc.invalidateQueries({ queryKey: ["checklist", projectId] }),
    ]);

  const capture = async () => {
    setBusy(true);
    try {
      await api.storeScreenshotCapture(projectId);
      await refreshShots();
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
    }
  };
  const importOwn = async () => {
    const sel = inTauri()
      ? await open({
          multiple: true,
          title: t("sec.publish.listing.importTitle"),
          filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg"] }],
        })
      : ["/demo/a.png"];
    const paths = Array.isArray(sel) ? sel : typeof sel === "string" ? [sel] : [];
    if (!paths.length) return;
    try {
      await api.storeScreenshotImport(projectId, paths);
      await refreshShots();
    } catch (e) {
      toastError(e, t);
    }
  };
  const save = async () => {
    try {
      await onMeta(draft);
      toast.success(t("sec.common.saved"));
    } catch (e) {
      toastError(e, t);
    }
  };

  return (
    <div className="space-y-5">
      <Card2 title={t("sec.publish.listing.title")}>
        <p className="t-body text-muted-foreground">{t("sec.publish.listing.intro")}</p>
        <Field
          label={t("sec.publish.listing.name")}
          help={`${t("sec.publish.listing.nameHelp")} (${l.title.length}/30)`}
          htmlFor="l-title"
        >
          <input
            id="l-title"
            value={l.title}
            maxLength={30}
            onChange={(e) => setL({ title: e.target.value })}
            className={inputCls}
          />
        </Field>
        <Field
          label={t("sec.publish.listing.short")}
          help={`${t("sec.publish.listing.shortHelp")} (${l.shortDescription.length}/80)`}
          htmlFor="l-short"
        >
          <input
            id="l-short"
            value={l.shortDescription}
            maxLength={80}
            onChange={(e) => setL({ shortDescription: e.target.value })}
            className={inputCls}
          />
        </Field>
        <Field
          label={t("sec.publish.listing.full")}
          help={`${t("sec.publish.listing.fullHelp")} (${l.fullDescription.length}/4000)`}
          htmlFor="l-full"
        >
          <textarea
            id="l-full"
            rows={6}
            value={l.fullDescription}
            maxLength={4000}
            onChange={(e) => setL({ fullDescription: e.target.value })}
            className={`${inputCls} h-auto py-2`}
          />
        </Field>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label={t("sec.publish.listing.category")} htmlFor="l-cat">
            <select
              id="l-cat"
              value={l.category}
              onChange={(e) => setL({ category: e.target.value })}
              className={inputCls}
            >
              <option value="" />
              {CATS.map((c) => (
                <option key={c} value={c}>
                  {t(`sec.publish.listing.categories.${c}`)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("sec.publish.listing.email")} htmlFor="l-email">
            <input
              id="l-email"
              type="email"
              value={l.contactEmail}
              onChange={(e) => setL({ contactEmail: e.target.value })}
              className={inputCls}
            />
          </Field>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button onClick={() => void save()}>{t("sec.publish.listing.saveListing")}</Button>
          <AskButton
            projectId={projectId}
            prompt={t("sec.publish.listing.writePrompt")}
            label={t("sec.publish.listing.write")}
            variant="outline"
          />
        </div>
      </Card2>

      <Card2 title={t("sec.publish.listing.screenshots")}>
        <p className="t-small text-muted-foreground">{t("sec.publish.listing.screenshotsHelp")}</p>
        <ul className="flex flex-wrap gap-3" aria-label={t("sec.publish.listing.screenshots")}>
          {(shots.data ?? []).map((n) => (
            <Thumb
              key={n}
              projectId={projectId}
              name={n}
              onDelete={() => void api.storeScreenshotDelete(projectId, n).then(refreshShots)}
            />
          ))}
        </ul>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="outline" disabled={busy || !previewUp} onClick={() => void capture()}>
            <Camera /> {busy ? t("sec.publish.listing.capturing") : t("sec.publish.listing.capture")}
          </Button>
          <Button variant="outline" onClick={() => void importOwn()}>
            <ImagePlus /> {t("sec.publish.listing.import")}
          </Button>
          {!previewUp && <span className="t-small text-muted-foreground">{t("sec.publish.listing.needPreview")}</span>}
        </div>
      </Card2>

      <Card2 title={t("sec.publish.listing.questions")}>
        <p className="t-small text-muted-foreground">{t("sec.publish.listing.questionsHelp")}</p>
        <ul className="divide-y">
          {QS.map((k) => (
            <li key={k} className="flex flex-wrap items-center justify-between gap-3 py-2.5">
              <span id={`q-${k}`} className="t-body">
                {t(`sec.publish.listing.q.${k}`)}
              </span>
              <div role="radiogroup" aria-labelledby={`q-${k}`} className="bg-muted inline-flex rounded-lg p-0.5">
                {[true, false].map((v) => (
                  <button
                    key={String(v)}
                    type="button"
                    role="radio"
                    aria-checked={draft.questionnaire[k] === v}
                    onClick={() => setQ(k, v)}
                    className={`t-small h-7 rounded-md px-3 font-medium transition-colors duration-[var(--dur)] ${draft.questionnaire[k] === v ? "bg-card text-foreground shadow-soft" : "text-muted-foreground hover:text-foreground"}`}
                  >
                    {t(v ? "sec.common.yes" : "sec.common.no")}
                  </button>
                ))}
              </div>
            </li>
          ))}
        </ul>
        {QS.every((k) => draft.questionnaire[k] !== null) && (
          <div className="bg-muted/50 space-y-2 rounded-xl p-3">
            <p className="t-body font-medium">{t("sec.publish.listing.summaryTitle")}</p>
            <ul className="t-small list-disc space-y-0.5 pl-5">
              {summaryKeys(draft.questionnaire).map((k) => (
                <li key={k}>{t(`sec.publish.listing.summary.${k}`)}</li>
              ))}
            </ul>
            <Check
              id="console-done"
              checked={draft.questionnaire.playConsoleDone}
              onChange={(v) => setDraft({ ...draft, questionnaire: { ...draft.questionnaire, playConsoleDone: v } })}
              label={t("sec.publish.listing.consoleDone")}
            />
            <Button
              size="sm"
              variant="outline"
              onClick={() =>
                void (inTauri()
                  ? openUrl("https://play.google.com/console")
                  : window.open("https://play.google.com/console", "_blank"))
              }
            >
              {t("sec.publish.listing.open")}
            </Button>
          </div>
        )}
        <Button onClick={() => void save()}>{t("sec.common.save")}</Button>
      </Card2>
    </div>
  );
}
