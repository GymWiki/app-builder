import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef } from "react";
import { api, onEvent } from "@/lib/ipc";
import { log } from "@/lib/log";

/** Ingebouwde terminal (xterm.js) gekoppeld aan een pty-sessie in Rust. */
export default function PtyTerminal({ ptyId, label }: { ptyId: string; label: string }) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let disposed = false;
    const cleanups: (() => void)[] = [];

    void (async () => {
      const [{ Terminal }, { FitAddon }] = await Promise.all([import("@xterm/xterm"), import("@xterm/addon-fit")]);
      if (disposed || !host.current) return;

      const term = new Terminal({
        fontFamily: 'ui-monospace, "Cascadia Code", "SF Mono", Menlo, Consolas, monospace',
        fontSize: 13,
        cursorBlink: true,
        convertEol: true,
        theme: { background: "#0f1117", foreground: "#e5e7eb", cursor: "#a5b4fc", selectionBackground: "#4f46e555" },
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.open(host.current);
      const sync = () => {
        try {
          fit.fit();
          void api.ptyResize(ptyId, term.cols, term.rows).catch(() => undefined);
        } catch {
          /* element nog niet zichtbaar */
        }
      };
      sync();

      const input = term.onData(
        (d) => void api.ptyWrite(ptyId, d).catch((e) => log("warn", `pty write: ${String(e)}`)),
      );
      const ro = new ResizeObserver(sync);
      ro.observe(host.current);
      const unData = await onEvent<{ id: string; data: string }>(
        "pty-data",
        (p) => p.id === ptyId && term.write(p.data),
      );
      const unExit = await onEvent<{ id: string }>("pty-exit", (p) => p.id === ptyId && term.write("\r\n"));

      cleanups.push(
        () => input.dispose(),
        () => ro.disconnect(),
        unData,
        unExit,
        () => term.dispose(),
      );
      if (disposed) cleanups.forEach((c) => c());
    })();

    return () => {
      disposed = true;
      cleanups.forEach((c) => c());
    };
  }, [ptyId]);

  return (
    <div ref={host} role="group" aria-label={label} className="h-56 overflow-hidden rounded-xl bg-[#0f1117] p-3" />
  );
}
