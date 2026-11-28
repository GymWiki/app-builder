import { FolderPlus, Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { toast } from "sonner";
import { Card, Panel, PanelHeader } from "@/components/ds/card";
import { EmptyState } from "@/components/ds/empty-state";
import { StatusBadge } from "@/components/ds/status-badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

/** Overzicht van het designsysteem: handig om thema's, staten en contrast te controleren. */
export default function DesignPage() {
  const { t } = useTranslation();
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-4xl space-y-10 px-6 py-10">
        <div className="flex items-center justify-between">
          <h1 className="t-display">{t("design.title")}</h1>
          <Button asChild variant="outline">
            <Link to="/settings">{t("common.back")}</Link>
          </Button>
        </div>

        <Block title={t("design.buttons")}>
          <div className="flex flex-wrap items-center gap-3">
            <Button>
              <Plus /> Primary
            </Button>
            <Button variant="secondary">Secondary</Button>
            <Button variant="outline">Outline</Button>
            <Button variant="ghost">Ghost</Button>
            <Button variant="destructive">
              <Trash2 /> Destructive
            </Button>
            <Button disabled>Disabled</Button>
            <Button size="icon" aria-label="Icon button">
              <Plus />
            </Button>
          </div>
        </Block>

        <Block title={t("design.badges")}>
          <div className="flex flex-wrap gap-2">
            <StatusBadge>Neutral</StatusBadge>
            <StatusBadge tone="accent">Accent</StatusBadge>
            <StatusBadge tone="success">Success</StatusBadge>
            <StatusBadge tone="warning">Warning</StatusBadge>
            <StatusBadge tone="danger">Danger</StatusBadge>
            <StatusBadge tone="info">Info</StatusBadge>
          </div>
        </Block>

        <Block title={t("design.cards")}>
          <div className="grid gap-4 sm:grid-cols-2">
            <Card className="p-5">Card</Card>
            <Card interactive className="p-5">
              Interactive card
            </Card>
            <Panel className="rounded-xl border sm:col-span-2">
              <PanelHeader
                title="Panel"
                actions={
                  <Button size="xs" variant="ghost">
                    Action
                  </Button>
                }
              />
              <p className="t-body text-muted-foreground p-4">Panel body</p>
            </Panel>
          </div>
        </Block>

        <Block title={t("design.toasts")}>
          <div className="flex gap-3">
            <Button variant="outline" onClick={() => toast.success(t("design.toastMessage"))}>
              {t("design.showToast")} · success
            </Button>
            <Button
              variant="outline"
              onClick={() => toast.error(t("design.toastMessage"), { description: t("design.emptyBody") })}
            >
              {t("design.showToast")} · error
            </Button>
          </div>
        </Block>

        <Block title={t("design.states")}>
          <div className="grid gap-4 sm:grid-cols-2">
            <Card>
              <EmptyState
                icon={FolderPlus}
                title={t("design.emptyTitle")}
                body={t("design.emptyBody")}
                action={<Button>{t("home.newProject")}</Button>}
                className="py-10"
              />
            </Card>
            <Card className="space-y-3 p-5">
              <Skeleton className="h-5 w-1/2" />
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-4 w-3/4" />
            </Card>
          </div>
        </Block>
      </div>
    </div>
  );
}

function Block({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h2 className="t-heading mb-3">{title}</h2>
      {children}
    </section>
  );
}
