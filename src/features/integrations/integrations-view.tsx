import { useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, KeyRound, TriangleAlert, XCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { StatusBadge } from "@/components/ds/status-badge";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/ipc";
import { CATEGORIES, type IntegrationStatus, type Manifest } from "@/lib/integrations";
import { toastError } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { SectionPage } from "../sections/shared";
import { ManifestIcon, useIntegrations, useRefreshIntegrations, type JumpTarget } from "./use-integrations";

type Pending = { kind: "off"; m: Manifest } | { kind: "needs"; m: Manifest; deps: Manifest[] } | null;

function badge(st: IntegrationStatus, t: (k: string) => string) {
  switch (st.state) {
    case "off":
      return { tone: "neutral" as const, text: t("sec.integrations.status.off"), Icon: null };
    case "complete":
      return { tone: "success" as const, text: t("sec.integrations.status.complete"), Icon: CheckCircle2 };
    case "warning":
      return { tone: "warning" as const, text: t("sec.integrations.status.warning"), Icon: TriangleAlert };
    case "incomplete":
      return { tone: "warning" as const, text: t("sec.integrations.status.incomplete"), Icon: TriangleAlert };
    default:
      return { tone: "danger" as const, text: t("sec.integrations.status.missing"), Icon: XCircle };
  }
}

export function IntegrationsView({
  projectId,
  target,
  onAddSecrets,
}: {
  projectId: string;
  target: JumpTarget | null;
  onAddSecrets: (t: Omit<JumpTarget, "nonce">) => void;
}) {
  const { t } = useTranslation();
  const q = useIntegrations(projectId);
  const refresh = useRefreshIntegrations(projectId);
  const qc = useQueryClient();
  const [pending, setPending] = useState<Pending>(null);
  const [busy, setBusy] = useState(false);
  const [flash, setFlash] = useState<string | null>(null);
  const cardRefs = useRef<Record<string, HTMLElement | null>>({});

  // Kort markeren van de kaart waar de gebruiker naartoe is gestuurd (bv. vanuit de checklist).
  useEffect(() => {
    if (!target || !q.data) return;
    const el = cardRefs.current[target.integration];
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
    el?.focus({ preventScroll: true });
    // eslint-disable-next-line react-hooks/set-state-in-effect -- een korte markering volgt direct uit de sprong
    setFlash(target.integration);
    const id = setTimeout(() => setFlash(null), 2200);
    return () => clearTimeout(id);
  }, [target, q.data]);

  const data = q.data;
  const statusOf = (id: string) => data?.statuses.find((s) => s.id === id);
  const manifestOf = (id: string) => data?.manifests.find((m) => m.id === id);

  const setEnabled = async (ids: string[], enabled: boolean, deleteSecrets = false) => {
    setBusy(true);
    try {
      for (const id of ids) await api.integrationSetEnabled(projectId, id, enabled, deleteSecrets);
      await refresh();
      await qc.invalidateQueries({ queryKey: ["integrations", projectId] });
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
      setPending(null);
    }
  };

  const toggle = (m: Manifest, on: boolean) => {
    if (!on) return setPending({ kind: "off", m });
    const deps = (m.afhankelijkheden ?? [])
      .map(manifestOf)
      .filter((d): d is Manifest => !!d && !statusOf(d.id)?.enabled);
    if (deps.length) return setPending({ kind: "needs", m, deps });
    void setEnabled([m.id], true).then(() => toast.success(t("sec.integrations.turnedOn", { name: t(m.naam) })));
  };

  return (
    <SectionPage title={t("sec.integrations.title")} intro={t("sec.integrations.intro")} className="max-w-4xl">
      {data &&
        CATEGORIES.map((cat) => {
          const list = data.manifests.filter((m) => m.categorie === cat);
          if (!list.length) return null;
          return (
            <section key={cat} aria-labelledby={`cat-${cat}`} className="space-y-3">
              <h3 id={`cat-${cat}`} className="t-heading">
                {t(`sec.integrations.categories.${cat}`)}
              </h3>
              <ul className="grid gap-3 sm:grid-cols-2">
                {list.map((m) => {
                  const st = statusOf(m.id)!;
                  const b = badge(st, t);
                  const hasFields = m.velden.length > 0;
                  const needsSecrets = st.enabled && st.missing.length > 0;
                  return (
                    <li
                      key={m.id}
                      id={`integration-${m.id}`}
                      ref={(el) => {
                        cardRefs.current[m.id] = el;
                      }}
                      tabIndex={-1}
                      className={cn(
                        "bg-card shadow-soft flex flex-col gap-3 rounded-xl border p-4 transition-shadow outline-none",
                        flash === m.id && "ring-primary ring-2",
                      )}
                    >
                      <div className="flex items-start gap-3">
                        <div className="bg-accent text-primary-text grid size-10 shrink-0 place-items-center rounded-lg">
                          <ManifestIcon name={m.icoon} className="size-5" />
                        </div>
                        <div className="min-w-0 flex-1">
                          <h4 className="t-body font-semibold">{t(m.naam)}</h4>
                          <p className="t-small text-muted-foreground">{t(m.beschrijving)}</p>
                        </div>
                        <Switch
                          checked={st.enabled}
                          disabled={busy}
                          onCheckedChange={(v) => toggle(m, v)}
                          aria-label={t("sec.integrations.switchLabel", { name: t(m.naam) })}
                        />
                      </div>
                      <div className="mt-auto flex flex-wrap items-center gap-2">
                        <StatusBadge tone={b.tone}>
                          {b.Icon && <b.Icon aria-hidden className="size-3" />}
                          {b.text}
                        </StatusBadge>
                        {st.enabled && hasFields && (
                          <Button
                            size="xs"
                            variant={needsSecrets ? "default" : "outline"}
                            onClick={() => onAddSecrets({ integration: m.id })}
                          >
                            <KeyRound />
                            {needsSecrets ? t("sec.integrations.addSecrets") : t("sec.integrations.openSecrets")}
                          </Button>
                        )}
                        {st.enabled && !hasFields && (
                          <span className="t-small text-muted-foreground">{t("sec.integrations.noSecretsNeeded")}</span>
                        )}
                      </div>
                      <p className="t-small text-muted-foreground">
                        {m.scope === "global" ? t("sec.secrets.scope.global") : t("sec.secrets.scope.project")}
                      </p>
                    </li>
                  );
                })}
              </ul>
            </section>
          );
        })}

      <AlertDialog open={pending !== null} onOpenChange={(o) => !o && !busy && setPending(null)}>
        <AlertDialogContent>
          {pending?.kind === "off" && (
            <>
              <AlertDialogHeader>
                <AlertDialogTitle>{t("sec.integrations.off.title", { name: t(pending.m.naam) })}</AlertDialogTitle>
                <AlertDialogDescription>
                  {t("sec.integrations.off.body")}
                  {pending.m.velden.some((f) => f.scope === "global") && ` ${t("sec.integrations.off.globalNote")}`}
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter className="sm:flex-col sm:items-stretch">
                <Button disabled={busy} onClick={() => void setEnabled([pending.m.id], false, false)}>
                  {t("sec.integrations.off.keep")}
                </Button>
                <Button
                  variant="destructive"
                  disabled={busy || pending.m.velden.every((f) => f.scope === "global")}
                  onClick={() => void setEnabled([pending.m.id], false, true)}
                >
                  {t("sec.integrations.off.delete")}
                </Button>
                <AlertDialogCancel disabled={busy}>{t("common.cancel")}</AlertDialogCancel>
              </AlertDialogFooter>
            </>
          )}
          {pending?.kind === "needs" && (
            <>
              <AlertDialogHeader>
                <AlertDialogTitle>{t("sec.integrations.needs.title", { name: t(pending.m.naam) })}</AlertDialogTitle>
                <AlertDialogDescription>
                  {t("sec.integrations.needs.body", {
                    name: t(pending.m.naam),
                    deps: pending.deps.map((d) => t(d.naam)).join(", "),
                  })}
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter className="sm:flex-col sm:items-stretch">
                <Button
                  disabled={busy}
                  onClick={() => void setEnabled([...pending.deps.map((d) => d.id), pending.m.id], true)}
                >
                  {t("sec.integrations.needs.both")}
                </Button>
                <Button variant="outline" disabled={busy} onClick={() => void setEnabled([pending.m.id], true)}>
                  {t("sec.integrations.needs.only")}
                </Button>
                <AlertDialogCancel disabled={busy}>{t("common.cancel")}</AlertDialogCancel>
              </AlertDialogFooter>
            </>
          )}
        </AlertDialogContent>
      </AlertDialog>
    </SectionPage>
  );
}
