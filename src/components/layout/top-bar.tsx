import { Settings } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Button } from "@/components/ui/button";
import { ThemeToggle } from "@/components/ds/theme-toggle";

export function Logo({ className }: { className?: string }) {
  return (
    <span className={className}>
      <img src="/studio.svg" alt="" className="size-7 rounded-lg" />
    </span>
  );
}

/** Bovenbalk van de startpagina en instellingen. */
export function AppTopBar({ children }: { children?: React.ReactNode }) {
  const { t } = useTranslation();
  return (
    <header className="bg-background/80 sticky top-0 z-10 flex h-14 shrink-0 items-center gap-3 border-b px-6 backdrop-blur">
      <Link
        to="/"
        className="flex items-center gap-2.5 rounded-lg font-semibold tracking-tight"
        aria-label={t("common.appName")}
      >
        <Logo />
        <span className="t-heading">{t("common.appName")}</span>
      </Link>
      <div className="flex-1">{children}</div>
      <ThemeToggle />
      <Button asChild variant="ghost" size="icon" aria-label={t("settings.title")} title={t("settings.title")}>
        <Link to="/settings">
          <Settings />
        </Link>
      </Button>
    </header>
  );
}
