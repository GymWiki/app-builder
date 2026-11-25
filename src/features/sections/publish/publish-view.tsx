import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/ipc";
import type { Meta, Project } from "@/lib/types";
import { cn } from "@/lib/utils";
import { SectionPage } from "../shared";
import { ChecklistTab } from "./checklist-tab";
import { LegalTab } from "./legal-tab";
import { ListingTab } from "./listing-tab";
import { ReleaseTab } from "./release-tab";
import { SigningTab } from "./signing-tab";
import { TestTab } from "./test-tab";
import { useChecklist } from "./use-checklist";

export const TABS = ["checklist", "test", "listing", "legal", "signing", "release"] as const;
export type PublishTab = (typeof TABS)[number];

/** Publiceren: één plek met de checklist-poort en alle stappen daarheen. */
export function PublishView({
  project,
  tab,
  onTab,
  goto,
}: {
  project: Project;
  tab: PublishTab;
  onTab: (t: PublishTab) => void;
  goto: (section: string) => void;
}) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const id = project.id;
  const evalQ = useChecklist(id);
  const metaQ = useQuery({ queryKey: ["meta", id], queryFn: () => api.metaGet(id) });
  const [saveError, setSaveError] = useState(false);

  const refresh = () =>
    Promise.all([
      qc.invalidateQueries({ queryKey: ["checklist", id] }),
      qc.invalidateQueries({ queryKey: ["project", id] }),
    ]);
  const saveMeta = async (m: Meta) => {
    setSaveError(false);
    try {
      await api.metaSave(id, m);
    } catch (e) {
      setSaveError(true);
      throw e;
    } finally {
      await qc.invalidateQueries({ queryKey: ["meta", id] });
      await refresh();
    }
  };

  const onNext = (next: string) => {
    if (next.startsWith("publish:")) {
      const target = next.slice(8);
      onTab((target === "confirm" ? "release" : target) as PublishTab);
    } else goto(next);
  };

  const meta = metaQ.data;
  const evaluation = evalQ.data;
  const docsReady = evaluation ? !evaluation.items.find((i) => i.key === "legal")?.missing.includes("docs") : false;

  return (
    <SectionPage title={t("sec.publish.title")} intro={t("sec.publish.intro")} className="max-w-4xl">
      <div
        role="tablist"
        aria-label={t("sec.publish.title")}
        className="bg-muted flex flex-wrap gap-0.5 rounded-xl p-1"
      >
        {TABS.map((k) => (
          <button
            key={k}
            type="button"
            role="tab"
            id={`ptab-${k}`}
            aria-selected={tab === k}
            aria-controls="ptab-panel"
            onClick={() => onTab(k)}
            className={cn(
              "t-small h-8 rounded-lg px-3 font-medium transition-colors duration-[var(--dur)]",
              tab === k ? "bg-card text-foreground shadow-soft" : "text-muted-foreground hover:text-foreground",
            )}
          >
            {t(`sec.publish.tabs.${k}`)}
          </button>
        ))}
      </div>
      <div id="ptab-panel" role="tabpanel" aria-labelledby={`ptab-${tab}`}>
        {!meta || !evaluation ? null : tab === "checklist" ? (
          <ChecklistTab evaluation={evaluation} onNext={onNext} />
        ) : tab === "test" ? (
          <TestTab projectId={id} meta={meta} onMeta={saveMeta} />
        ) : tab === "listing" ? (
          <ListingTab
            key={JSON.stringify(meta.listing) + JSON.stringify(meta.questionnaire)}
            projectId={id}
            meta={meta}
            onMeta={saveMeta}
          />
        ) : tab === "legal" ? (
          <LegalTab
            key={JSON.stringify(meta.legal)}
            projectId={id}
            meta={meta}
            onMeta={saveMeta}
            docsReady={docsReady}
          />
        ) : tab === "signing" ? (
          <SigningTab projectId={id} meta={meta} onMeta={saveMeta} />
        ) : (
          <ReleaseTab project={project} meta={meta} evaluation={evaluation} onMeta={saveMeta} onDone={refresh} />
        )}
        {saveError && (
          <p role="alert" className="text-destructive t-small mt-2">
            {t("sec.common.loadError")}
          </p>
        )}
      </div>
    </SectionPage>
  );
}
