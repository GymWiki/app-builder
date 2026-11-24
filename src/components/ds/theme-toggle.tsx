import { Monitor, Moon, Sun } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useUi, type ThemePref } from "@/stores/ui";

const OPTIONS: { value: ThemePref; icon: typeof Sun }[] = [
  { value: "light", icon: Sun },
  { value: "system", icon: Monitor },
  { value: "dark", icon: Moon },
];

/** Segmentschakelaar licht / systeem / donker. */
export function ThemeToggle({ className }: { className?: string }) {
  const { t } = useTranslation();
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  return (
    <div
      role="radiogroup"
      aria-label={t("theme.label")}
      className={cn("bg-muted inline-flex rounded-lg p-0.5", className)}
    >
      {OPTIONS.map(({ value, icon: Icon }) => (
        <button
          key={value}
          type="button"
          role="radio"
          aria-checked={theme === value}
          aria-label={t(`theme.${value}`)}
          title={t(`theme.${value}`)}
          onClick={() => setTheme(value)}
          className={cn(
            "grid size-7 place-items-center rounded-md transition-colors duration-[var(--dur)]",
            theme === value ? "bg-card text-foreground shadow-soft" : "text-muted-foreground hover:text-foreground",
          )}
        >
          <Icon className="size-4" />
        </button>
      ))}
    </div>
  );
}
