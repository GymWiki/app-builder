import type { ScreenNode } from "./types";

/// Past een werkelijke route (`/item/7`) bij een patroon (`/item/:id`)?
export function routeMatches(pattern: string, actual: string): boolean {
  const norm = (r: string) => r.split(/[?#]/)[0]!.replace(/\/+$/, "") || "/";
  const p = norm(pattern).split("/");
  const a = norm(actual).split("/");
  return p.length === a.length && p.every((seg, i) => seg.startsWith(":") || seg === a[i]);
}

/// De pagina die bij een route hoort: eerst een exacte voorbeeldroute, dan een patroon met parameters.
export function pageForRoute(nodes: ScreenNode[], route: string): ScreenNode | null {
  const withRoute = nodes.filter((n) => n.route);
  return (
    withRoute.find((n) => (n.previewRoute ?? n.route) === route) ??
    withRoute.find((n) => routeMatches(n.route!, route)) ??
    null
  );
}
