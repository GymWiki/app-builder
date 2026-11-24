import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, FolderOpen } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";
import { Card } from "@/components/ds/card";
import { LanguageSwitch } from "@/components/ds/language-switch";
import { ThemeToggle } from "@/components/ds/theme-toggle";
import { Logo } from "@/components/layout/top-bar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { qk, useAppInfo, useSettings } from "@/hooks";
import { api, inTauri } from "@/lib/ipc";
import { toastError } from "@/lib/toast";

const ORG_RE = /^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$/;

export function SettingsPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { data: info } = useAppInfo();
  const { data: settings } = useSettings();
  const orgId = useId();
  const [org, setOrg] = useState<string | null>(null);
  const orgValue = org ?? settings?.org ?? "com.example";
  const orgValid = ORG_RE.test(orgValue);

  const save = async (key: "projects_dir" | "org", value: string) => {
    try {
      await api.settingsSet(key, value);
      await Promise.all([
        qc.invalidateQueries({ queryKey: qk.settings }),
        qc.invalidateQueries({ queryKey: qk.appInfo }),
      ]);
      toast.success(t("settings.saved"));
    } catch (e) {
      toastError(e, t);
    }
  };

  const pickFolder = async () => {
    if (!inTauri()) return;
    const dir = await open({
      directory: true,
      multiple: false,
      title: t("settings.pickFolder"),
      defaultPath: info?.projectsDir,
    });
    if (typeof dir === "string") await save("projects_dir", dir);
  };

  return (
    <div className="h-full overflow-y-auto">
      <header className="bg-background/80 sticky top-0 z-10 flex h-14 items-center gap-3 border-b px-6 backdrop-blur">
        <Button asChild variant="ghost" size="icon" aria-label={t("common.back")}>
          <Link to="/">
            <ArrowLeft />
          </Link>
        </Button>
        <Logo />
        <h1 className="t-heading">{t("settings.title")}</h1>
      </header>

      <main className="mx-auto max-w-2xl space-y-6 px-6 py-8">
        <Section title={t("settings.appearance")}>
          <div className="flex flex-wrap items-center justify-between gap-4">
            <span className="t-body font-medium">{t("theme.label")}</span>
            <ThemeToggle />
          </div>
          <div className="flex flex-wrap items-center justify-between gap-4">
            <span className="t-body font-medium">{t("language.label")}</span>
            <LanguageSwitch />
          </div>
        </Section>

        <Section title={t("settings.projectsFolder")}>
          <p className="text-muted-foreground t-small">{t("settings.projectsFolderHelp")}</p>
          <div className="flex items-center gap-2">
            <code
              className="bg-muted t-small min-w-0 flex-1 truncate rounded-lg px-3 py-2 font-mono"
              title={info?.projectsDir}
            >
              {info?.projectsDir ?? "…"}
            </code>
            <Button variant="outline" onClick={() => void pickFolder()}>
              <FolderOpen /> {t("settings.change")}
            </Button>
          </div>
        </Section>

        <Section title={t("settings.org")}>
          <p className="text-muted-foreground t-small">{t("settings.orgHelp")}</p>
          <div className="flex items-start gap-2">
            <div className="flex-1">
              <Label htmlFor={orgId} className="sr-only">
                {t("settings.org")}
              </Label>
              <Input
                id={orgId}
                value={orgValue}
                onChange={(e) => setOrg(e.target.value.trim().toLowerCase())}
                aria-invalid={!orgValid}
                className="h-10 rounded-lg font-mono"
              />
              {!orgValid && (
                <p role="alert" className="text-destructive t-small mt-1.5">
                  {t("settings.orgInvalid")}
                </p>
              )}
            </div>
            <Button
              disabled={!orgValid || orgValue === (settings?.org ?? "com.example")}
              onClick={() => void save("org", orgValue)}
            >
              {t("common.save")}
            </Button>
          </div>
        </Section>

        <Section title={t("settings.tools")}>
          <div className="flex flex-wrap items-center justify-between gap-4">
            <p className="text-muted-foreground t-small max-w-sm">{t("settings.toolsHelp")}</p>
            <Button variant="outline" onClick={() => navigate("/setup")}>
              {t("settings.runSetup")}
            </Button>
          </div>
        </Section>

        <Section title={t("settings.about")}>
          <p className="t-body">
            {t("settings.version", { version: info?.version ?? "…", os: info ? `${info.os} (${info.arch})` : "…" })}
          </p>
          {info?.logDir && (
            <div className="flex items-center gap-2">
              <code
                className="bg-muted t-small min-w-0 flex-1 truncate rounded-lg px-3 py-2 font-mono"
                title={info.logDir}
              >
                {info.logDir}
              </code>
              <Button
                variant="outline"
                onClick={() => inTauri() && void revealItemInDir(info.logDir!).catch((e) => toastError(e, t))}
              >
                {t("settings.showInFolder")}
              </Button>
            </div>
          )}
          <Link to="/design" className="text-primary-text t-small underline-offset-4 hover:underline">
            {t("settings.designSystem")}
          </Link>
        </Section>
      </main>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Card className="space-y-4 p-6">
      <h2 className="t-heading">{title}</h2>
      {children}
    </Card>
  );
}
