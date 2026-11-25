import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Meta } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Card2, Check, Field, SectionPage, inputCls, useAsk } from "./shared";

const PROVIDERS = ["none", "local", "supabase", "firebase"] as const;
const METHODS = ["email", "google", "apple", "anonymous"] as const;

export function DataView({ projectId }: { projectId: string }) {
  const { t } = useTranslation();
  const meta = useQuery({ queryKey: ["meta", projectId], queryFn: () => api.metaGet(projectId) });
  if (!meta.data) return <SectionPage title={t("sec.data.title")}>{null}</SectionPage>;
  // Het formulier start met de opgeslagen waarden (geen state-synchronisatie in effecten nodig).
  return <DataForm projectId={projectId} initial={meta.data} />;
}

function DataForm({ projectId, initial }: { projectId: string; initial: Meta }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const hasToken = useQuery({
    queryKey: ["secret", projectId, "supabase_token"],
    queryFn: () => api.secretHas(projectId, "supabase_token"),
  });
  const [draft, setDraft] = useState<Meta>(initial);
  const [token, setToken] = useState("");
  const { ask, busy } = useAsk(projectId);

  const set = (patch: Partial<Meta["data"]>) => setDraft({ ...draft, data: { ...draft.data, ...patch } });
  const save = async (m: Meta = draft) => {
    try {
      await api.metaSave(projectId, m);
      await qc.invalidateQueries({ queryKey: ["meta", projectId] });
      await qc.invalidateQueries({ queryKey: ["checklist", projectId] });
      toast.success(t("sec.common.saved"));
    } catch (e) {
      toastError(e, t);
    }
  };
  const toggleMethod = (m: string, on: boolean) =>
    set({ authMethods: on ? [...draft.data.authMethods, m] : draft.data.authMethods.filter((x) => x !== m) });
  const saveToken = async () => {
    try {
      await api.secretSet(projectId, "supabase_token", token);
      setToken(""); // de waarde verdwijnt direct uit de UI
      await qc.invalidateQueries({ queryKey: ["secret", projectId, "supabase_token"] });
      toast.success(t("sec.data.tokenSaved"));
    } catch (e) {
      toastError(e, t);
    }
  };
  const setup = async () => {
    await save();
    try {
      ask(await api.dataSetupPrompt(projectId));
    } catch (e) {
      toastError(e, t);
    }
  };
  const p = draft.data.provider;
  const accounts = draft.data.authMethods.some((m) => m !== "anonymous") || draft.questionnaire.hasAccounts === true;

  return (
    <SectionPage title={t("sec.data.title")} intro={t("sec.data.intro")}>
      <Card2 title={t("sec.data.provider")}>
        <div role="radiogroup" aria-label={t("sec.data.provider")} className="grid gap-2.5 sm:grid-cols-2">
          {PROVIDERS.map((id) => (
            <button
              key={id}
              type="button"
              role="radio"
              aria-checked={p === id}
              onClick={() => set({ provider: id })}
              className={cn(
                "rounded-xl border p-3 text-left transition-colors duration-[var(--dur)]",
                p === id ? "border-primary bg-accent/60 ring-primary/30 ring-2" : "hover:bg-muted/60",
              )}
            >
              <span className="t-body block font-medium">{t(`sec.data.providers.${id}.name`)}</span>
              <span className="t-small text-muted-foreground block">{t(`sec.data.providers.${id}.desc`)}</span>
            </button>
          ))}
        </div>

        {p === "supabase" && (
          <div className="space-y-3 pt-2">
            <Field label={t("sec.data.supabaseRef")} help={t("sec.data.supabaseRefHelp")} htmlFor="sb-ref">
              <input
                id="sb-ref"
                value={draft.data.supabaseProjectRef}
                onChange={(e) => set({ supabaseProjectRef: e.target.value.trim() })}
                className={cn(inputCls, "font-mono")}
              />
            </Field>
            <Field label={t("sec.data.supabaseUrl")} htmlFor="sb-url">
              <input
                id="sb-url"
                value={draft.data.supabaseUrl}
                placeholder="https://xxxx.supabase.co"
                onChange={(e) => set({ supabaseUrl: e.target.value.trim() })}
                className={cn(inputCls, "font-mono")}
              />
            </Field>
            <Field label={t("sec.data.token")} help={t("sec.data.tokenHelp")} htmlFor="sb-token">
              {hasToken.data ? (
                <div className="flex items-center gap-3">
                  <span className="text-success t-small font-medium">✓ {t("sec.data.tokenSaved")}</span>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      void api
                        .secretDelete(projectId, "supabase_token")
                        .then(() => qc.invalidateQueries({ queryKey: ["secret", projectId, "supabase_token"] }))
                    }
                  >
                    {t("sec.data.tokenRemove")}
                  </Button>
                </div>
              ) : (
                <div className="flex gap-2">
                  <input
                    id="sb-token"
                    type="password"
                    autoComplete="off"
                    value={token}
                    placeholder={t("sec.data.tokenPlaceholder")}
                    onChange={(e) => setToken(e.target.value)}
                    className={cn(inputCls, "font-mono")}
                  />
                  <Button disabled={!token.trim()} onClick={() => void saveToken()}>
                    {t("sec.data.tokenSave")}
                  </Button>
                </div>
              )}
            </Field>
          </div>
        )}
        {p === "firebase" && <p className="t-small text-muted-foreground pt-1">{t("sec.data.firebaseNote")}</p>}
      </Card2>

      {(p === "supabase" || p === "firebase") && (
        <Card2 title={t("sec.data.auth")}>
          <p className="t-small text-muted-foreground">{t("sec.data.authHelp")}</p>
          <div className="space-y-2.5">
            {METHODS.map((m) => (
              <Check
                key={m}
                id={`auth-${m}`}
                checked={draft.data.authMethods.includes(m)}
                onChange={(v) => toggleMethod(m, v)}
                label={t(`sec.data.methods.${m}`)}
              />
            ))}
          </div>
        </Card2>
      )}

      {accounts && (
        <Card2>
          <Check
            id="acct-del"
            checked={draft.legal.accountDeletion}
            onChange={(v) => setDraft({ ...draft, legal: { ...draft.legal, accountDeletion: v } })}
            label={t("sec.data.accountDeletion")}
            help={t("sec.data.accountDeletionHelp")}
          />
        </Card2>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={() => void save()}>{t("sec.common.save")}</Button>
        {p !== "" && p !== "none" && (
          <>
            <Button variant="outline" disabled={busy} onClick={() => void setup()}>
              {t("sec.data.setup")}
            </Button>
            <span className="t-small text-muted-foreground">{t("sec.data.setupHint")}</span>
          </>
        )}
      </div>
    </SectionPage>
  );
}
