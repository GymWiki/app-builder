import { useQuery } from "@tanstack/react-query";
import {
  ChevronRight,
  Folder as FolderIcon,
  FolderOpen,
  FolderPlus,
  Home,
  LayoutGrid,
  MoreHorizontal,
  RefreshCw,
} from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { AskButton } from "@/features/sections/shared";
import { api } from "@/lib/ipc";
import type { Screen } from "@/lib/types";
import { cn } from "@/lib/utils";
import {
  addFolder,
  deleteFolder,
  flatten,
  loadFolders,
  moveFolder,
  moveScreen,
  prune,
  renameFolder,
  saveFolders,
  toggleFolder,
  type Folder,
  type FolderState,
} from "./screens-folders";
import { treeRows } from "./screens-order";

/** Een gekozen pagina; `route` is het pad waar de preview naartoe springt (als het scherm er een heeft). */
export type PickedScreen = Screen & { route?: string | null };

type Draft = { kind: "new"; parent: string | null } | { kind: "rename"; id: string };
type Drag = { type: "screen" | "folder"; id: string };

/** Boom van de pagina's van de app, met mappen die de gebruiker zelf maakt. Een pagina kiezen = preview + chat gebruiken die pagina. */
export function ScreensTree({
  projectId,
  selected,
  onSelect,
}: {
  projectId: string;
  selected: string | null;
  onSelect: (s: PickedScreen | null) => void;
}) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["screens", projectId], queryFn: () => api.screensGraph(projectId) });
  const [state, setState] = useState<FolderState>(() => loadFolders(projectId));
  const [draft, setDraft] = useState<Draft | null>(null);
  const [toDelete, setToDelete] = useState<Folder | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [over, setOver] = useState<string | "root" | null>(null);

  const update = (fn: (s: FolderState) => FolderState) =>
    setState((s) => {
      const next = fn(s);
      if (next !== s) saveFolders(projectId, next);
      return next;
    });

  const { order, byClass, home, linked } = useMemo(() => {
    if (!q.data)
      return { order: [] as string[], byClass: new Map<string, PickedScreen>(), home: null, linked: new Set<string>() };
    const { rows, unlinked } = treeRows(q.data);
    const nodes = new Map(q.data.nodes.map((n) => [n.class, n]));
    const by = new Map<string, PickedScreen>();
    for (const r of [...rows, ...unlinked])
      by.set(r.class, { class: r.class, file: r.file, line: r.line, route: nodes.get(r.class)?.route });
    return {
      order: [...rows, ...unlinked].map((r) => r.class),
      byClass: by,
      home: q.data.home,
      linked: new Set(rows.map((r) => r.class)),
    };
  }, [q.data]);

  const view = useMemo(() => (q.data ? prune(state, order) : state), [state, order, q.data]);
  const items = flatten(view, order);
  const empty = !q.isPending && order.length === 0 && view.folders.length === 0;
  const pick = (c: string) => {
    const sc = byClass.get(c);
    if (sc) onSelect(selected === c ? null : sc);
  };

  const drop = (target: string | null) => {
    if (drag?.type === "screen") update((s) => moveScreen(s, drag.id, target));
    if (drag?.type === "folder") update((s) => moveFolder(s, drag.id, target));
    setDrag(null);
    setOver(null);
  };
  const dropProps = (target: string | null) => ({
    onDragOver: (e: React.DragEvent) => {
      if (!drag) return;
      e.preventDefault();
      e.stopPropagation();
      setOver(target ?? "root");
    },
    onDrop: (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      drop(target);
    },
  });

  const folderChoices = (skip?: string) =>
    flatten({ ...view, closed: [] }, [])
      .filter((i) => i.kind === "folder" && i.folder.id !== skip)
      .map((i) => i as Extract<typeof i, { kind: "folder" }>);

  const moveMenu = (current: string | null, onPick: (f: string | null) => void, skip?: string) => (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>{t("sec.screens.tree.moveTo")}</DropdownMenuSubTrigger>
      <DropdownMenuSubContent>
        <DropdownMenuItem disabled={current === null} onSelect={() => onPick(null)}>
          {t("sec.screens.tree.noFolder")}
        </DropdownMenuItem>
        {folderChoices(skip).map((f) => (
          <DropdownMenuItem
            key={f.folder.id}
            disabled={current === f.folder.id}
            onSelect={() => onPick(f.folder.id)}
            style={{ paddingLeft: 8 + f.depth * 12 }}
          >
            <FolderIcon aria-hidden className="size-4" /> {f.folder.name}
          </DropdownMenuItem>
        ))}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  );

  const nameInput = (initial: string, onDone: (v: string | null) => void, depth: number) => (
    <li style={{ paddingLeft: depth * 14 }}>
      <form
        className="flex items-center gap-1.5 px-1 py-1"
        onSubmit={(e) => {
          e.preventDefault();
          onDone(String(new FormData(e.currentTarget).get("name") ?? ""));
        }}
      >
        <FolderIcon aria-hidden className="text-muted-foreground size-4 shrink-0" />
        <input
          name="name"
          autoFocus
          defaultValue={initial}
          maxLength={40}
          aria-label={t("sec.screens.tree.folderName")}
          placeholder={t("sec.screens.tree.folderName")}
          onKeyDown={(e) => e.key === "Escape" && onDone(null)}
          onBlur={(e) => onDone(e.currentTarget.value)}
          className="border-input bg-background t-body focus-visible:ring-ring/50 min-w-0 flex-1 rounded-md border px-2 py-1 outline-none focus-visible:ring-2"
        />
      </form>
    </li>
  );

  const finish = (v: string | null) => {
    const d = draft;
    setDraft(null);
    if (!d || v === null) return;
    if (d.kind === "new") update((s) => addFolder(s, v, d.parent));
    else update((s) => renameFolder(s, d.id, v));
  };

  const folderRow = (f: Folder, depth: number, open: boolean, count: number) => {
    if (draft?.kind === "rename" && draft.id === f.id) return <div key={f.id}>{nameInput(f.name, finish, depth)}</div>;
    const Icon = open ? FolderOpen : FolderIcon;
    return (
      <li
        key={f.id}
        style={{ paddingLeft: depth * 14 }}
        draggable
        onDragStart={(e) => {
          e.dataTransfer.effectAllowed = "move";
          setDrag({ type: "folder", id: f.id });
        }}
        onDragEnd={() => {
          setDrag(null);
          setOver(null);
        }}
        {...dropProps(f.id)}
      >
        <div
          className={cn(
            "group hover:bg-muted/60 flex items-center rounded-lg transition-colors duration-[var(--dur)]",
            over === f.id && "bg-accent ring-primary ring-1",
          )}
        >
          <button
            type="button"
            aria-expanded={open}
            onClick={() => update((s) => toggleFolder(s, f.id))}
            className="t-body flex min-w-0 flex-1 items-center gap-1.5 rounded-lg px-1.5 py-1.5 text-left font-medium"
          >
            <ChevronRight
              aria-hidden
              className={cn("text-muted-foreground size-3.5 shrink-0 transition-transform", open && "rotate-90")}
            />
            <Icon aria-hidden className="text-primary-text size-4 shrink-0" />
            <span className="truncate">{f.name}</span>
            <span className="t-small text-muted-foreground ml-auto pl-1 tabular-nums">{count}</span>
          </button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={t("sec.screens.tree.folderMenu", { name: f.name })}
                className="mr-1 opacity-60 group-hover:opacity-100 focus-visible:opacity-100"
              >
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => setDraft({ kind: "new", parent: f.id })}>
                {t("sec.screens.tree.newInside")}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => setDraft({ kind: "rename", id: f.id })}>
                {t("sec.screens.tree.rename")}
              </DropdownMenuItem>
              {moveMenu(f.parent, (p) => update((s) => moveFolder(s, f.id, p)), f.id)}
              <DropdownMenuSeparator />
              <DropdownMenuItem variant="destructive" onSelect={() => setToDelete(f)}>
                {t("sec.screens.tree.delete")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
        {draft?.kind === "new" && draft.parent === f.id && open && <ul>{nameInput("", finish, 1)}</ul>}
      </li>
    );
  };

  const screenRow = (c: string, depth: number) => {
    const on = selected === c;
    const isHome = c === home;
    const folder = view.placed[c] ?? null;
    return (
      <li
        key={c}
        style={{ paddingLeft: depth * 14 }}
        draggable
        onDragStart={(e) => {
          e.dataTransfer.effectAllowed = "move";
          setDrag({ type: "screen", id: c });
        }}
        onDragEnd={() => {
          setDrag(null);
          setOver(null);
        }}
      >
        <div
          className={cn(
            "group flex items-center rounded-lg border transition-colors duration-[var(--dur)]",
            on ? "border-primary bg-accent/60" : "hover:bg-muted/60 border-transparent",
          )}
        >
          <button
            type="button"
            aria-pressed={on}
            title={byClass.get(c)?.file}
            onClick={() => pick(c)}
            className="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-left"
          >
            {isHome ? (
              <Home aria-hidden className="text-primary-text size-4 shrink-0" />
            ) : (
              <LayoutGrid
                aria-hidden
                className={cn("size-4 shrink-0", on ? "text-primary-text" : "text-muted-foreground")}
              />
            )}
            <span className="t-body truncate font-medium">{c}</span>
            {!linked.has(c) && (
              <span className="t-small text-muted-foreground ml-auto shrink-0 pl-1">
                {t("sec.screens.tree.notLinked")}
              </span>
            )}
          </button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={t("sec.screens.tree.pageMenu", { name: c })}
                className="mr-1 opacity-60 group-hover:opacity-100 focus-visible:opacity-100"
              >
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuLabel className="truncate">{c}</DropdownMenuLabel>
              {moveMenu(folder, (f) => update((s) => moveScreen(s, c, f)))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </li>
    );
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-11 shrink-0 items-center gap-1 border-b px-3">
        <h2 className="t-heading mr-auto truncate">{t("sec.screens.tree.title")}</h2>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("sec.screens.tree.newFolder")}
          title={t("sec.screens.tree.newFolder")}
          onClick={() => setDraft({ kind: "new", parent: null })}
        >
          <FolderPlus />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("sec.screens.refresh")}
          title={t("sec.screens.refresh")}
          onClick={() => void q.refetch()}
        >
          <RefreshCw className={cn(q.isFetching && "animate-spin")} />
        </Button>
      </div>

      <div className={cn("min-h-0 flex-1 overflow-y-auto p-2", over === "root" && "bg-accent/30")} {...dropProps(null)}>
        <p className="t-small text-muted-foreground px-1 pb-2">{t("sec.screens.tree.hint")}</p>
        {empty ? (
          <div className="bg-muted/50 rounded-xl border border-dashed p-4 text-center">
            <LayoutGrid aria-hidden className="text-muted-foreground mx-auto mb-2 size-5" />
            <p className="t-body font-medium">{t("sec.screens.none")}</p>
            <p className="t-small text-muted-foreground mt-1">{t("sec.screens.noneHint")}</p>
          </div>
        ) : (
          <ul className="space-y-0.5" aria-label={t("sec.screens.tree.title")}>
            {draft?.kind === "new" && draft.parent === null && nameInput("", finish, 0)}
            {items.map((i) =>
              i.kind === "folder" ? folderRow(i.folder, i.depth, i.open, i.count) : screenRow(i.class, i.depth),
            )}
          </ul>
        )}
      </div>

      <div className="flex shrink-0 flex-wrap gap-2 border-t p-2">
        <AskButton
          projectId={projectId}
          prompt={t("sec.screens.addPrompt")}
          label={t("sec.screens.addScreen")}
          variant="outline"
        />
        {order.length > 1 && (
          <AskButton
            projectId={projectId}
            prompt={t("sec.screens.linkPrompt")}
            label={t("sec.screens.linkScreens")}
            variant="outline"
          />
        )}
      </div>

      <AlertDialog open={toDelete !== null} onOpenChange={(o) => !o && setToDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("sec.screens.tree.deleteTitle", { name: toDelete?.name })}</AlertDialogTitle>
            <AlertDialogDescription>{t("sec.screens.tree.deleteBody")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (toDelete) update((s) => deleteFolder(s, toDelete.id));
                setToDelete(null);
              }}
            >
              {t("sec.screens.tree.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
