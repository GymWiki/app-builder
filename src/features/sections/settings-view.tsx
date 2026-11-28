import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Switch } from "@/components/ui/switch";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Applied, PlatformSettings, Project } from "@/lib/types";
import { cn } from "@/lib/utils";
import { AppliedNotice, Card2, Field, SectionPage, inputCls } from "./shared";

const BUNDLE_RE = /^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$/;
const PERMS = ["internet", "camera", "location", "microphone", "photos", "notifications"] as const;
const IOS_REASON_KEYS = new Set(["camera", "location", "microphone", "photos"]);

export function SettingsView({ project }: { project: Project }) {
  const { t } = useTranslation();
  const platform = useQuery({ queryKey: ["platform", project.id], queryFn: () => api.platformGet(project.id) });
  // Het resultaat staat hier (niet in het formulier), zodat het zichtbaar blijft als het formulier opnieuw start.
  const [result, setResult] = useState<Applied | null>(null);
  if (!platform.data) return <SectionPage title={t("sec.settings.title")}>{null}</SectionPage>;
  return (
    <SettingsForm
      key={project.version + project.buildNumber + project.name + JSON.stringify(platform.data)}
      project={project}
      p={platform.data}
      result={result}
      setResult={setResult}
    />
  );
}

function SettingsForm({
  project,
  p,
  result,
  setResult,
}: {
  project: Project;
  p: PlatformSettings;
  result: Applied | null;
  setResult: (r: Applied | null) => void;
}) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const id = project.id;
  const [name, setName] = useState(p.appName ?? project.name);
  const [bundle, setBundle] = useState(p.androidApplicationId ?? p.bundleId ?? project.bundleId);
  const [minAndroid, setMinAndroid] = useState(p.minAndroid ? String(p.minAndroid) : "");
  const [minIos, setMinIos] = useState(p.minIos ?? "");
  const [reasons, setReasons] = useState<Record<string, string>>(
    Object.fromEntries(p.permissions.map((x) => [x.key, x.ios_reason ?? ""])),
  );

  const refresh = async () => {
    await Promise.all([
      qc.invalidateQueries({ queryKey: ["platform", id] }),
      qc.invalidateQueries({ queryKey: ["project", id] }),
      qc.invalidateQueries({ queryKey: ["checklist", id] }),
    ]);
  };
  const run = async (f: () => Promise<Applied | void>) => {
    try {
      const r = await f();
      setResult(r ?? { applied: [t("sec.common.saved")], skipped: [] });
      await refresh();
    } catch (e) {
      toastError(e, t);
    }
  };
  // Wijzigingen worden toegepast zodra je het veld verlaat (geen aparte opslaanknop); alleen als er echt iets veranderd is.
  const saveMinOs = () => {
    const a = minAndroid ? Number(minAndroid) : null;
    const i = /^\d+(\.\d+)?$/.test(minIos) ? minIos : null;
    if ((a === null && i === null) || (a === (p.minAndroid ?? null) && i === (p.minIos ?? null))) return;
    void run(() => api.platformSetMinOs(id, a, i));
  };
  const bundleValid = BUNDLE_RE.test(bundle);
  const exampleId = bundle.startsWith("com.example.");

  return (
    <SectionPage title={t("sec.settings.title")} intro={t("sec.settings.intro")}>
      <Card2>
        <Field label={t("sec.settings.name")} help={t("sec.settings.nameHelp")} htmlFor="s-name">
          <input
            id="s-name"
            value={name}
            maxLength={30}
            onChange={(e) => setName(e.target.value)}
            onBlur={() => name.trim() && name !== (p.appName ?? project.name) && void run(() => api.platformSetName(id, name))}
            className={inputCls}
          />
        </Field>
        <Field label={t("sec.settings.bundle")} help={t("sec.settings.bundleHelp")} htmlFor="s-bundle">
          <input
            id="s-bundle"
            value={bundle}
            onChange={(e) => setBundle(e.target.value.trim())}
            onBlur={() =>
              bundleValid && bundle !== (p.androidApplicationId ?? p.bundleId) && void run(() => api.platformSetBundle(id, bundle))
            }
            aria-invalid={!bundleValid}
            className={cn(inputCls, "font-mono")}
          />
          {!bundleValid && (
            <p role="alert" className="text-destructive t-small">
              {t("sec.settings.bundleInvalid")}
            </p>
          )}
          {exampleId && <p className="text-warning t-small">{t("sec.settings.bundleDefault")}</p>}
        </Field>
      </Card2>

      <Card2>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label={t("sec.settings.minAndroid")} htmlFor="s-android">
            <input
              id="s-android"
              value={minAndroid}
              inputMode="numeric"
              placeholder="24"
              onChange={(e) => setMinAndroid(e.target.value.replace(/\D/g, ""))}
              onBlur={saveMinOs}
              className={inputCls}
            />
          </Field>
          <Field label={t("sec.settings.minIos")} htmlFor="s-ios">
            <input
              id="s-ios"
              value={minIos}
              placeholder="13.0"
              onChange={(e) => setMinIos(e.target.value.trim())}
              onBlur={saveMinOs}
              className={inputCls}
            />
          </Field>
        </div>
      </Card2>

      <Card2 title={t("sec.settings.permissions")}>
        <p className="t-small text-muted-foreground">{t("sec.settings.permissionsHelp")}</p>
        <ul className="divide-y">
          {PERMS.map((key) => {
            const st = p.permissions.find((x) => x.key === key);
            const on = st?.enabled ?? false;
            return (
              <li key={key} className="space-y-2 py-3">
                <div className="flex items-center justify-between gap-3">
                  <label htmlFor={`perm-${key}`} className="t-body font-medium">
                    {t(`sec.settings.permission.${key}`)}
                  </label>
                  <Switch
                    id={`perm-${key}`}
                    checked={on}
                    onCheckedChange={(v) =>
                      void run(() =>
                        api.platformSetPermission(id, key, v, reasons[key] || t("sec.settings.iosReasonDefault")),
                      )
                    }
                  />
                </div>
                {on && IOS_REASON_KEYS.has(key) && (
                  <input
                    aria-label={`${t(`sec.settings.permission.${key}`)}: ${t("sec.settings.iosReason")}`}
                    value={reasons[key] ?? ""}
                    placeholder={t("sec.settings.iosReason")}
                    onChange={(e) => setReasons({ ...reasons, [key]: e.target.value })}
                    onBlur={() =>
                      (reasons[key] ?? "") !== (st?.ios_reason ?? "") &&
                      void run(() => api.platformSetPermission(id, key, true, reasons[key] ?? ""))
                    }
                    className={inputCls}
                  />
                )}
              </li>
            );
          })}
        </ul>
      </Card2>

      <AppliedNotice
        result={result}
        projectId={id}
        askPrompt={`${t("sec.settings.title")}: ${t("sec.common.skippedHelp")}`}
      />
    </SectionPage>
  );
}
