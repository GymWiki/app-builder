import { useQuery, useQueryClient } from "@tanstack/react-query";
import { qk } from "@/hooks";
import { api } from "@/lib/ipc";
import type { ToolId, ToolStatus } from "@/lib/types";

export type RowId = "git" | "flutter" | "android" | "claude" | "claudeLogin";
export type RowState = "checking" | "ok" | "missing" | "outdated" | "warning";

export interface Row {
  id: RowId;
  state: RowState;
  status?: ToolStatus;
}

const opts = { staleTime: Infinity, gcTime: Infinity } as const;

export function useToolchain() {
  const qc = useQueryClient();
  const git = useQuery({ queryKey: qk.tool("git"), queryFn: api.checkGit, ...opts });
  const claude = useQuery({ queryKey: qk.tool("claude"), queryFn: api.checkClaude, ...opts });
  const flutter = useQuery({ queryKey: qk.tool("flutter"), queryFn: api.checkFlutter, ...opts });
  const claudeInstalled = claude.data && claude.data.state !== "missing";
  const login = useQuery({ queryKey: qk.login, queryFn: api.claudeLoginStatus, enabled: !!claudeInstalled, ...opts });

  const of = (q: { isPending: boolean; isError: boolean }, s?: ToolStatus): RowState =>
    q.isPending ? "checking" : q.isError ? "warning" : (s?.state ?? "warning");

  const rows: Row[] = [
    { id: "git", state: of(git, git.data), status: git.data },
    { id: "flutter", state: of(flutter, flutter.data?.flutter), status: flutter.data?.flutter },
    { id: "android", state: of(flutter, flutter.data?.android), status: flutter.data?.android },
    { id: "claude", state: of(claude, claude.data), status: claude.data },
    {
      id: "claudeLogin",
      state:
        !claudeInstalled && !claude.isPending
          ? "missing"
          : login.isPending && claudeInstalled
            ? "checking"
            : login.data
              ? "ok"
              : "missing",
    },
  ];

  const recheck = (id: RowId | ToolId) => {
    const key = id === "android" ? "flutter" : id === "claudeLogin" ? "login" : id;
    if (key === "login") return qc.invalidateQueries({ queryKey: qk.login });
    const invalidate = qc.invalidateQueries({ queryKey: qk.tool(key) });
    // Als Claude (opnieuw) is geïnstalleerd, ook de loginstatus opnieuw bekijken.
    if (key === "claude") void qc.invalidateQueries({ queryKey: qk.login });
    return invalidate;
  };

  return {
    rows,
    recheck,
    doneCount: rows.filter((r) => r.state === "ok").length,
    allOk: rows.every((r) => r.state === "ok"),
  };
}
