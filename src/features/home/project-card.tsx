import { FolderOpen, MoreHorizontal, Trash2 } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Card } from "@/components/ds/card";
import { StatusBadge, type Tone } from "@/components/ds/status-badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { inTauri } from "@/lib/ipc";
import { timeAgo } from "@/lib/format";
import type { Project, ProjectStatus } from "@/lib/types";
import { toastError } from "@/lib/toast";
import { TemplateIcon } from "./templates";

const TONE: Record<ProjectStatus, Tone> = { draft: "neutral", testing: "info", ready: "success", published: "accent" };

export function ProjectCard({ project, onRemove }: { project: Project; onRemove: (p: Project) => void }) {
  const { t, i18n } = useTranslation();

  const reveal = async () => {
    try {
      if (inTauri()) await revealItemInDir(project.path);
    } catch (e) {
      toastError(e, t);
    }
  };

  return (
    <Card interactive className="group relative flex flex-col gap-4 p-5">
      <div className="flex items-start justify-between gap-3">
        <div className="bg-accent text-accent-foreground grid size-11 shrink-0 place-items-center rounded-xl">
          <TemplateIcon id={project.template} className="size-5" strokeWidth={1.75} />
        </div>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              className="relative z-10 -mt-1 -mr-1.5 opacity-60 group-hover:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
              aria-label={t("project.actions", { name: project.name })}
            >
              <MoreHorizontal />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-52 rounded-xl">
            <DropdownMenuItem asChild>
              <Link to={`/project/${project.id}`}>{t("project.open")}</Link>
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={reveal}>
              <FolderOpen /> {t("project.showInFolder")}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={() => onRemove(project)}>
              <Trash2 /> {t("project.remove")}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <div className="min-w-0">
        <h2 className="t-heading truncate">
          {/* Hele kaart klikbaar via een uitgerekte link; het menu staat erboven (z-10). */}
          <Link
            to={`/project/${project.id}`}
            className="rounded-sm after:absolute after:inset-0 after:rounded-xl focus-visible:outline-2"
          >
            {project.name}
          </Link>
        </h2>
        <p className="text-muted-foreground t-small mt-0.5">
          {t("project.modified", { when: timeAgo(project.updatedAt, i18n.language) })}
        </p>
      </div>

      <div className="mt-auto flex items-center justify-between gap-2">
        <StatusBadge tone={TONE[project.status]}>{t(`status.${project.status}`)}</StatusBadge>
        <span className="text-muted-foreground t-small tabular-nums">
          {t("project.version", { version: project.version })}
        </span>
      </div>
    </Card>
  );
}

export function ProjectCardSkeleton() {
  return (
    <Card className="flex flex-col gap-4 p-5" aria-hidden>
      <div className="bg-muted size-11 animate-pulse rounded-xl" />
      <div className="space-y-2">
        <div className="bg-muted h-4 w-2/3 animate-pulse rounded" />
        <div className="bg-muted h-3 w-1/3 animate-pulse rounded" />
      </div>
      <div className="mt-2 flex justify-between">
        <div className="bg-muted h-5 w-16 animate-pulse rounded-full" />
        <div className="bg-muted h-4 w-14 animate-pulse rounded" />
      </div>
    </Card>
  );
}
