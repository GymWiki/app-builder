import { useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { friendlyError } from "@/lib/errors";
import { api, inTauri } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Meta } from "@/lib/types";
import { AskButton, Card2, Check, Field, inputCls } from "../shared";

const open = (url: string) => (inTauri() ? openUrl(url) : window.open(url, "_blank", "noopener"));
const Status = ({ ok, yes, no }: { ok: boolean; yes: string; no: string }) => (
  <span className={ok ? "text-success t-small font-medium" : "text-muted-foreground t-small"}>
    {ok ? `✓ ${yes}` : no}
  </span>
);

export function SigningTab({
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
  const st = useQuery({ queryKey: ["signing", projectId], queryFn: () => api.signingStatus(projectId) });
  const [owner, setOwner] = useState("");
  const [org, setOrg] = useState("");
  const [country, setCountry] = useState("NL");
  const [busy, setBusy] = useState(false);
  const [key, setKey] = useState("");
  const [iosNote, setIosNote] = useState<string | null>(null);
  const s = st.data;
  const refresh = async () => {
    await Promise.all([
      qc.invalidateQueries({ queryKey: ["signing", projectId] }),
      qc.invalidateQueries({ queryKey: ["checklist", projectId] }),
    ]);
  };
  const create = async () => {
    setBusy(true);
    try {
      await api.signingCreateKeystore(projectId, owner, org, country);
      await refresh();
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
    }
  };
  const saveKey = async () => {
    try {
      await api.secretSet(projectId, "play_service_account_json", key);
      setKey(""); // verdwijnt direct uit de UI
      await refresh();
      toast.success(t("sec.publish.signing.keySaved"));
    } catch (e) {
      toastError(e, t);
    }
  };
  const gradlePrompt = useQuery({ queryKey: ["signingPrompt"], queryFn: api.signingPrompt, staleTime: Infinity });
  const steps = t("sec.publish.signing.playSteps", { returnObjects: true }) as string[];

  return (
    <div className="space-y-5">
      <p className="t-body text-muted-foreground">{t("sec.publish.signing.intro")}</p>
      <Card2 title={t("sec.publish.signing.keystore")}>
        <Status
          ok={!!s?.keystore}
          yes={t("sec.publish.signing.keystoreReady")}
          no={t("sec.publish.signing.keystoreMissing")}
        />
        {s?.keystore ? (
          <>
            {s.keystorePath && (
              <p className="t-small text-muted-foreground font-mono break-all">
                {t("sec.publish.signing.keystorePath", { path: s.keystorePath })}
              </p>
            )}
            <p className="text-warning t-small font-medium">{t("sec.publish.signing.backup")}</p>
          </>
        ) : (
          <div className="space-y-3">
            {s && !s.keytool && (
              <p role="alert" className="text-destructive t-small">
                {t("sec.publish.signing.noKeytool")}
              </p>
            )}
            <div className="grid gap-3 sm:grid-cols-3">
              <Field label={t("sec.publish.signing.owner")} htmlFor="ks-owner">
                <input id="ks-owner" value={owner} onChange={(e) => setOwner(e.target.value)} className={inputCls} />
              </Field>
              <Field label={t("sec.publish.signing.organisation")} htmlFor="ks-org">
                <input id="ks-org" value={org} onChange={(e) => setOrg(e.target.value)} className={inputCls} />
              </Field>
              <Field label={t("sec.publish.signing.country")} htmlFor="ks-country">
                <input
                  id="ks-country"
                  value={country}
                  maxLength={2}
                  onChange={(e) => setCountry(e.target.value.toUpperCase())}
                  className={inputCls}
                />
              </Field>
            </div>
            <Button disabled={busy || !owner.trim()} onClick={() => void create()}>
              {busy ? t("sec.publish.signing.creating") : t("sec.publish.signing.create")}
            </Button>
          </div>
        )}
      </Card2>

      <Card2 title={t("sec.publish.signing.gradle")}>
        <Status
          ok={!!s?.gradleConfigured}
          yes={t("sec.publish.signing.gradleReady")}
          no={t("sec.publish.signing.gradleMissing")}
        />
        {!s?.gradleConfigured && gradlePrompt.data && (
          <AskButton
            projectId={projectId}
            prompt={gradlePrompt.data}
            label={t("sec.publish.signing.gradleAsk")}
            variant="outline"
          />
        )}
      </Card2>

      <Card2 title={t("sec.publish.signing.playTitle")}>
        <p className="t-small text-muted-foreground">{t("sec.publish.signing.playHelp")}</p>
        <ol className="t-body list-decimal space-y-1.5 pl-5">
          {steps.map((x, i) => (
            <li key={i}>{x}</li>
          ))}
        </ol>
        <Button size="sm" variant="outline" onClick={() => void open("https://play.google.com/console")}>
          {t("sec.publish.signing.playOpen")}
        </Button>
        <Field label={t("sec.publish.signing.keyLabel")} htmlFor="play-key">
          {s?.playKey ? (
            <div className="flex items-center gap-3">
              <span className="text-success t-small font-medium">✓ {t("sec.publish.signing.keySaved")}</span>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void api.secretDelete(projectId, "play_service_account_json").then(refresh)}
              >
                {t("sec.publish.signing.keyRemove")}
              </Button>
            </div>
          ) : (
            <div className="space-y-2">
              <textarea
                id="play-key"
                rows={4}
                value={key}
                autoComplete="off"
                spellCheck={false}
                onChange={(e) => setKey(e.target.value)}
                className={`${inputCls} h-auto py-2 font-mono`}
              />
              <Button disabled={!key.trim()} onClick={() => void saveKey()}>
                {t("sec.publish.signing.keySave")}
              </Button>
            </div>
          )}
        </Field>
        <Check
          id="play-ready"
          checked={meta.publishing.playAccountReady}
          onChange={(v) => void onMeta({ ...meta, publishing: { ...meta.publishing, playAccountReady: v } })}
          label={t("sec.publish.signing.accountReady")}
        />
      </Card2>

      <Card2 title={t("sec.publish.signing.fastlane")}>
        <Status
          ok={!!s?.fastlane}
          yes={t("sec.publish.signing.fastlaneReady")}
          no={t("sec.publish.signing.fastlaneMissing")}
        />
        <p className="t-small text-muted-foreground">
          <strong>{t("sec.publish.signing.closedTitle")}.</strong> {t("sec.publish.signing.closedBody")}
        </p>
      </Card2>

      <Card2 title={t("sec.publish.signing.iosTitle")}>
        <p className="t-small text-muted-foreground">{t("sec.publish.signing.iosBody")}</p>
        <Button
          size="sm"
          variant="outline"
          onClick={() => void api.publishIos().catch((e) => setIosNote(friendlyError(e, t).title))}
        >
          {t("sec.publish.signing.iosCheck")}
        </Button>
        {iosNote && (
          <p role="status" className="t-small text-muted-foreground">
            {iosNote}
          </p>
        )}
      </Card2>
    </div>
  );
}
