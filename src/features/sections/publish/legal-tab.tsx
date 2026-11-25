import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { toastError } from "@/lib/toast";
import type { Meta } from "@/lib/types";
import { AskButton, Card2, Check, Field, inputCls } from "../shared";

export function LegalTab({
  projectId,
  meta,
  onMeta,
  docsReady,
}: {
  projectId: string;
  meta: Meta;
  onMeta: (m: Meta) => Promise<void>;
  docsReady: boolean;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(meta);
  const l = draft.legal;
  const set = (p: Partial<Meta["legal"]>) => setDraft({ ...draft, legal: { ...l, ...p } });
  const steps = t("sec.publish.legal.hostSteps", { returnObjects: true }) as string[];
  const urlOk = (u: string) => u === "" || /^https:\/\/\S+$/.test(u);
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
      <Card2 title={t("sec.publish.legal.title")}>
        <p className="t-body text-muted-foreground">{t("sec.publish.legal.intro")}</p>
        <div className="flex flex-wrap items-center gap-3">
          <AskButton
            projectId={projectId}
            prompt={t("sec.publish.legal.generatePrompt")}
            label={t("sec.publish.legal.generate")}
          />
          <span className={docsReady ? "text-success t-small font-medium" : "text-muted-foreground t-small"}>
            {docsReady ? `✓ ${t("sec.publish.legal.docsReady")}` : t("sec.publish.legal.docsMissing")}
          </span>
        </div>
        <p className="t-small text-muted-foreground">{t("sec.publish.legal.disclaimer")}</p>
      </Card2>

      <Card2 title={t("sec.publish.legal.hostTitle")}>
        <ol className="t-body list-decimal space-y-1.5 pl-5">
          {steps.map((s, i) => (
            <li key={i}>{s}</li>
          ))}
        </ol>
        <Field label={t("sec.publish.legal.privacyUrl")} help={t("sec.publish.legal.urlHelp")} htmlFor="privacy-url">
          <input
            id="privacy-url"
            value={l.privacyUrl}
            placeholder="https://"
            aria-invalid={!urlOk(l.privacyUrl)}
            onChange={(e) => set({ privacyUrl: e.target.value.trim(), hostedChecked: false })}
            className={inputCls}
          />
        </Field>
        <Field label={t("sec.publish.legal.termsUrl")} htmlFor="terms-url">
          <input
            id="terms-url"
            value={l.termsUrl}
            placeholder="https://"
            aria-invalid={!urlOk(l.termsUrl)}
            onChange={(e) => set({ termsUrl: e.target.value.trim(), hostedChecked: false })}
            className={inputCls}
          />
        </Field>
        <Check
          id="hosted"
          checked={l.hostedChecked}
          disabled={!l.privacyUrl || !urlOk(l.privacyUrl) || !urlOk(l.termsUrl)}
          onChange={(v) => set({ hostedChecked: v })}
          label={t("sec.publish.legal.hostedChecked")}
        />
        <Check
          id="legal-acct"
          checked={l.accountDeletion}
          onChange={(v) => set({ accountDeletion: v })}
          label={t("sec.publish.legal.accountDeletion")}
          help={t("sec.publish.legal.accountDeletionHelp")}
        />
        <Button disabled={!urlOk(l.privacyUrl) || !urlOk(l.termsUrl)} onClick={() => void save()}>
          {t("sec.publish.legal.saveLegal")}
        </Button>
      </Card2>
    </div>
  );
}
