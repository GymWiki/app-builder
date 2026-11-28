import { useQuery } from "@tanstack/react-query";
import { CheckCircle2, Download, KeyRound, ShieldAlert, TriangleAlert, Upload, XCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { EmptyState } from "@/components/ds/empty-state";
import { StatusBadge } from "@/components/ds/status-badge";
import { Button } from "@/components/ui/button";
import { toAppError } from "@/lib/errors";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, inTauri } from "@/lib/ipc";
import { parseRejection, type IntegrationStatus, type Manifest, type TestOutcome } from "@/lib/integrations";
import { toastError } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { AskButton, Card2, SectionPage, inputCls } from "../sections/shared";
import { ConfirmDialog } from "./confirm-dialog";
import { SecretField } from "./secret-field";
import { ManifestIcon, useIntegrations, useRefreshIntegrations, type JumpTarget } from "./use-integrations";

function relTime(iso: string | null, lang: string): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? null : d.toLocaleString(lang, { dateStyle: "medium", timeStyle: "short" });
}

function stateBadge(st: IntegrationStatus, hasTest: boolean, t: (k: string) => string) {
  if (st.state === "complete") {
    const tested = hasTest && st.lastTestOk === true;
    return {
      tone: "success" as const,
      Icon: CheckCircle2,
      text: t(tested ? "sec.secrets.state.completeTested" : "sec.secrets.state.complete"),
    };
  }
  if (st.state === "warning") {
    const expiring = st.expiresInDays !== null && st.expiresInDays < 14;
    return {
      tone: "warning" as const,
      Icon: TriangleAlert,
      text: t(expiring ? "sec.secrets.state.expires" : "sec.secrets.state.failed"),
    };
  }
  if (st.state === "incomplete")
    return { tone: "warning" as const, Icon: TriangleAlert, text: t("sec.secrets.state.incomplete") };
  return { tone: "danger" as const, Icon: XCircle, text: t("sec.secrets.state.missing") };
}

function Section({
  projectId,
  manifest,
  status,
  target,
  onChanged,
}: {
  projectId: string;
  manifest: Manifest;
  status: IntegrationStatus;
  target: JumpTarget | null;
  onChanged: () => Promise<unknown>;
}) {
  const { t, i18n } = useTranslation();
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<TestOutcome | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const ref = useRef<HTMLElement>(null);
  const b = stateBadge(status, !!manifest.testVerbinding, t);

  // Spring naar dit kopje (of een veld) en markeer het kort.
  useEffect(() => {
    if (!target || target.integration !== manifest.id) return;
    const el = (target.field && document.getElementById(`secret-${manifest.id}-${target.field}`)) || ref.current;
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
    (el as HTMLElement | null)?.focus({ preventScroll: true });
    // eslint-disable-next-line react-hooks/set-state-in-effect -- een korte markering volgt direct uit de sprong
    setFlash(target.field ?? "_section");
    const id = setTimeout(() => setFlash(null), 2200);
    return () => clearTimeout(id);
  }, [target, manifest.id]);

  const test = async () => {
    setTesting(true);
    setResult(null);
    try {
      setResult(await api.secretTest(projectId, manifest.id));
      await onChanged();
    } catch (e) {
      toastError(e, t);
    } finally {
      setTesting(false);
    }
  };

  const lastUsed = status.fields
    .filter((f) => f.lastUsed)
    .sort((a, c) => (c.lastUsed ?? "").localeCompare(a.lastUsed ?? ""))[0];
  const usedBy = [
    ...manifest.checklistItems.map((k) => t(`sec.publish.items.${k}.title`)),
    ...[...new Set(status.fields.map((f) => f.usedBy).filter(Boolean))].map((u) =>
      t(`sec.secrets.usedBy.${u}`, { defaultValue: u }),
    ),
  ];
  const outcome =
    result ??
    (status.lastTested
      ? { ok: status.lastTestOk === true, code: status.lastTestCode ?? "", detail: null, expiresAt: status.expiresAt }
      : null);

  return (
    <section
      id={`secrets-${manifest.id}`}
      ref={ref}
      tabIndex={-1}
      aria-labelledby={`secrets-${manifest.id}-h`}
      className={cn(
        "bg-card shadow-soft space-y-4 rounded-xl border p-5 transition-shadow outline-none",
        flash === "_section" && "ring-primary ring-2",
      )}
    >
      <header className="flex flex-wrap items-center gap-3">
        <div className="bg-accent text-primary-text grid size-9 place-items-center rounded-lg">
          <ManifestIcon name={manifest.icoon} className="size-4" />
        </div>
        <h3 id={`secrets-${manifest.id}-h`} className="t-heading">
          {t(manifest.naam)}
        </h3>
        <StatusBadge tone={b.tone}>
          <b.Icon aria-hidden className="size-3" />
          {b.text}
        </StatusBadge>
        <span className="t-small text-muted-foreground ml-auto">
          {manifest.scope === "global" ? t("sec.secrets.scope.global") : t("sec.secrets.scope.project")}
        </span>
      </header>

      {status.expiresInDays !== null && (
        <p
          role="status"
          className={cn("t-small font-medium", status.expiresInDays < 14 ? "text-warning" : "text-muted-foreground")}
        >
          {status.expiresInDays < 0
            ? t("sec.secrets.expired")
            : t("sec.secrets.expiresIn", { days: status.expiresInDays })}
        </p>
      )}

      {manifest.velden.length === 0 ? (
        <p className="t-body text-muted-foreground">{t("sec.integrations.noSecretsNeeded")}</p>
      ) : (
        <div className="space-y-3">
          {manifest.velden.map((f) => (
            <SecretField
              key={f.id}
              projectId={projectId}
              manifest={manifest}
              field={f}
              status={status.fields.find((x) => x.id === f.id)!}
              highlighted={flash === f.id}
              onChanged={onChanged}
            />
          ))}
        </div>
      )}

      <footer className="flex flex-wrap items-center gap-x-6 gap-y-2 border-t pt-3">
        {manifest.testVerbinding && (
          <Button
            size="sm"
            variant="outline"
            disabled={testing || status.missing.length > 0}
            onClick={() => void test()}
          >
            {testing ? t("sec.secrets.testing") : t("sec.secrets.test")}
          </Button>
        )}
        <div aria-live="polite" className="t-small">
          {outcome && (
            <span className={outcome.ok ? "text-success" : "text-destructive"}>
              {t(`sec.secrets.testResult.${outcome.code}`, {
                defaultValue: outcome.ok ? t("sec.secrets.testResult.ok") : t("sec.secrets.testResult.unexpected"),
                who: outcome.detail ?? "",
              })}
              {!result && status.lastTested && (
                <span className="text-muted-foreground">
                  {" "}
                  · {t("sec.secrets.lastTested", { when: relTime(status.lastTested, i18n.language) })}
                </span>
              )}
            </span>
          )}
          {!outcome && manifest.testVerbinding && (
            <span className="text-muted-foreground">{t("sec.secrets.notTested")}</span>
          )}
        </div>
        <dl className="t-small text-muted-foreground ml-auto flex flex-wrap gap-x-5 gap-y-1">
          <div className="flex gap-1">
            <dt>{t("sec.secrets.lastUsed")}:</dt>
            <dd>{lastUsed ? relTime(lastUsed.lastUsed, i18n.language) : t("sec.secrets.never")}</dd>
          </div>
          {usedBy.length > 0 && (
            <div className="flex gap-1">
              <dt>{t("sec.secrets.usedBy.label")}:</dt>
              <dd>{usedBy.join(", ")}</dd>
            </div>
          )}
        </dl>
      </footer>
    </section>
  );
}

function Leaks({ projectId, target }: { projectId: string; target: JumpTarget | null }) {
  const { t } = useTranslation();
  const refresh = useRefreshIntegrations(projectId);
  const q = useQuery({ queryKey: ["leaks", projectId], queryFn: () => api.leaksScan(projectId), staleTime: 0 });
  const [moving, setMoving] = useState<string | null>(null);
  const [prompts, setPrompts] = useState<Record<string, string>>({});
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    if (target?.integration === "leaks") ref.current?.scrollIntoView({ block: "start", behavior: "smooth" });
  }, [target]);
  useEffect(() => {
    const list = q.data ?? [];
    void Promise.all(
      list
        .filter((l) => !l.movable)
        .map(async (l) => [l.file, await api.leakFixPrompt(l.file, l.line, l.kind)] as const),
    ).then((e) => setPrompts(Object.fromEntries(e)));
  }, [q.data]);
  const leaks = q.data ?? [];
  if (!leaks.length) return null;
  const move = async (file: string) => {
    try {
      await api.leakMoveToVault(projectId, file);
      toast.success(t("sec.leaks.moved"));
      await Promise.all([q.refetch(), refresh()]);
    } catch (e) {
      const err = toAppError(e);
      if (err.code === "rejected")
        toast.error(
          t(`sec.secrets.errors.${parseRejection(err.message).code}`, {
            defaultValue: t("sec.secrets.errors.generic"),
          }),
        );
      else toastError(e, t);
    } finally {
      setMoving(null);
    }
  };
  return (
    <section
      id="secrets-leaks"
      ref={ref}
      tabIndex={-1}
      aria-labelledby="leaks-h"
      className="border-destructive/50 bg-destructive/5 space-y-3 rounded-xl border p-5 outline-none"
    >
      <h3 id="leaks-h" className="t-heading flex items-center gap-2">
        <ShieldAlert aria-hidden className="text-destructive size-5" /> {t("sec.leaks.title")}
      </h3>
      <p className="t-body">{t("sec.leaks.body")}</p>
      <ul className="space-y-2">
        {leaks.map((l) => (
          <li key={`${l.file}:${l.kind}`} className="bg-card rounded-lg border p-3">
            <p className="t-body font-medium">
              <span className="font-mono">{l.file}</span>
              {l.line > 0 && <span className="text-muted-foreground"> · {t("sec.leaks.line", { n: l.line })}</span>}
            </p>
            <p className="t-small text-muted-foreground">
              {t(`sec.leaks.kind.${l.kind}`, { defaultValue: t("sec.leaks.kind.token") })}
            </p>
            {l.tracked && <p className="t-small text-warning font-medium">{t("sec.leaks.tracked")}</p>}
            <div className="mt-2 flex flex-wrap gap-2">
              {l.movable ? (
                <Button size="sm" onClick={() => setMoving(l.file)}>
                  {t("sec.leaks.move")}
                </Button>
              ) : (
                prompts[l.file] && (
                  <AskButton
                    projectId={projectId}
                    prompt={prompts[l.file] ?? ""}
                    label={t("sec.leaks.askClaude")}
                    variant="outline"
                  />
                )
              )}
            </div>
          </li>
        ))}
      </ul>
      <ConfirmDialog
        open={moving !== null}
        title={t("sec.leaks.confirmTitle")}
        body={t("sec.leaks.confirmBody", { file: moving ?? "" })}
        confirmLabel={t("sec.leaks.move")}
        destructive
        onConfirm={() => moving && void move(moving)}
        onCancel={() => setMoving(null)}
      />
    </section>
  );
}

function Backup({ projectId, onChanged }: { projectId: string; onChanged: () => Promise<unknown> }) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<null | "export" | "import">(null);
  const [pass, setPass] = useState("");
  const [pass2, setPass2] = useState("");
  const [file, setFile] = useState<File | null>(null);
  const [pickedPath, setPickedPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const reset = () => {
    setMode(null);
    setPass("");
    setPass2("");
    setFile(null);
    setPickedPath(null);
    setErr(null);
  };
  const filters = [{ name: "Studio back-up", extensions: ["studiovault"] }];
  const pick = async () => {
    const p = await openDialog({ multiple: false, filters });
    if (typeof p === "string") setPickedPath(p);
  };
  const run = async () => {
    setBusy(true);
    setErr(null);
    try {
      if (mode === "export") {
        if (inTauri()) {
          // Desktop: het besturingssysteem vraagt waar het bestand moet komen; de back-up gaat rechtstreeks naar die plek.
          const path = await saveDialog({ defaultPath: "studio-secrets.studiovault", filters });
          if (!path) return;
          await api.vaultExportFile(projectId, pass, path);
        } else {
          const b64 = await api.vaultExport(projectId, pass);
          const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
          const url = URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }));
          const a = document.createElement("a");
          a.href = url;
          a.download = "studio-secrets.studiovault";
          a.click();
          setTimeout(() => URL.revokeObjectURL(url), 5000);
        }
        toast.success(t("sec.secrets.backup.exported"));
      } else if (inTauri() && pickedPath) {
        const n = await api.vaultImportFile(projectId, pass, pickedPath);
        await onChanged();
        toast.success(t("sec.secrets.backup.imported", { n }));
      } else if (file) {
        const buf = new Uint8Array(await file.arrayBuffer());
        let s = "";
        for (let i = 0; i < buf.length; i += 0x8000) s += String.fromCharCode(...buf.subarray(i, i + 0x8000));
        const n = await api.vaultImport(projectId, pass, btoa(s));
        await onChanged();
        toast.success(t("sec.secrets.backup.imported", { n }));
      }
      reset();
    } catch (e) {
      const er = toAppError(e);
      if (er.code === "rejected")
        setErr(
          t(`sec.secrets.errors.${parseRejection(er.message).code}`, {
            detail: parseRejection(er.message).detail ?? "",
            defaultValue: t("sec.secrets.errors.generic"),
          }),
        );
      else toastError(e, t);
    } finally {
      setBusy(false);
    }
  };
  const mismatch = mode === "export" && pass !== pass2;
  return (
    <Card2 title={t("sec.secrets.backup.title")}>
      <p className="t-body text-muted-foreground">{t("sec.secrets.backup.why")}</p>
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" onClick={() => setMode("export")}>
          <Download /> {t("sec.secrets.backup.export")}
        </Button>
        <Button variant="outline" onClick={() => setMode("import")}>
          <Upload /> {t("sec.secrets.backup.import")}
        </Button>
      </div>
      {mode && (
        <form
          className="bg-muted/50 space-y-3 rounded-lg p-4"
          onSubmit={(e) => {
            e.preventDefault();
            void run();
          }}
        >
          <p className="t-small">
            {t(mode === "export" ? "sec.secrets.backup.exportHelp" : "sec.secrets.backup.importHelp")}
          </p>
          {mode === "import" &&
            (inTauri() ? (
              <div className="flex flex-wrap items-center gap-2">
                <Button type="button" variant="outline" size="sm" onClick={() => void pick()}>
                  {t("sec.secrets.backup.choose")}
                </Button>
                {pickedPath && <span className="t-small font-mono">{pickedPath.split(/[\\/]/).pop()}</span>}
              </div>
            ) : (
              <input
                type="file"
                accept=".studiovault"
                aria-label={t("sec.secrets.backup.file")}
                onChange={(e) => setFile(e.target.files?.[0] ?? null)}
                className="t-small"
              />
            ))}
          <input
            type="password"
            autoComplete="off"
            aria-label={t("sec.secrets.backup.passphrase")}
            placeholder={t("sec.secrets.backup.passphrase")}
            value={pass}
            onChange={(e) => setPass(e.target.value)}
            className={inputCls}
          />
          {mode === "export" && (
            <input
              type="password"
              autoComplete="off"
              aria-label={t("sec.secrets.backup.repeat")}
              placeholder={t("sec.secrets.backup.repeat")}
              value={pass2}
              onChange={(e) => setPass2(e.target.value)}
              className={inputCls}
            />
          )}
          {mismatch && pass2 && (
            <p role="alert" className="text-destructive t-small">
              {t("sec.secrets.backup.mismatch")}
            </p>
          )}
          {err && (
            <p role="alert" className="text-destructive t-small">
              {err}
            </p>
          )}
          <div className="flex gap-2">
            <Button
              type="submit"
              disabled={
                busy ||
                !pass ||
                mismatch ||
                (mode === "export" && !pass2) ||
                (mode === "import" && !file && !pickedPath)
              }
            >
              {mode === "export" ? t("sec.secrets.backup.doExport") : t("sec.secrets.backup.doImport")}
            </Button>
            <Button type="button" variant="ghost" onClick={reset}>
              {t("common.cancel")}
            </Button>
          </div>
        </form>
      )}
    </Card2>
  );
}

export function SecretsView({
  projectId,
  target,
  onGoIntegrations,
}: {
  projectId: string;
  target: JumpTarget | null;
  onGoIntegrations: () => void;
}) {
  const { t } = useTranslation();
  const q = useIntegrations(projectId);
  const refresh = useRefreshIntegrations(projectId);
  const data = q.data;
  const active = (data?.manifests ?? []).filter((m) => data?.statuses.find((s) => s.id === m.id)?.enabled);

  if (data && active.length === 0) {
    return (
      <div className="h-full overflow-y-auto">
        <Leaks projectId={projectId} target={target} />
        <EmptyState
          icon={KeyRound}
          title={t("sec.secrets.empty.title")}
          body={t("sec.secrets.empty.body")}
          action={<Button onClick={onGoIntegrations}>{t("sec.secrets.empty.action")}</Button>}
        />
      </div>
    );
  }
  return (
    <SectionPage title={t("sec.secrets.title")} intro={t("sec.secrets.intro")} className="max-w-4xl">
      <Leaks projectId={projectId} target={target} />
      <div role="note" className="bg-muted/50 t-small flex gap-3 rounded-xl border p-4">
        <ShieldAlert aria-hidden className="text-primary-text mt-0.5 size-4 shrink-0" />
        <p>{t("sec.secrets.publicVsSecret")}</p>
      </div>
      {data &&
        active.map((m) => (
          <Section
            key={m.id}
            projectId={projectId}
            manifest={m}
            status={data.statuses.find((s) => s.id === m.id)!}
            target={target}
            onChanged={refresh}
          />
        ))}
      <Backup projectId={projectId} onChanged={refresh} />
    </SectionPage>
  );
}
