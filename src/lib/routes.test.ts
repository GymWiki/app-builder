import { describe, expect, it } from "vitest";
import { pageForRoute, routeMatches } from "./routes";
import type { ScreenNode } from "./types";

const n = (class_: string, route: string | null, previewRoute: string | null = route): ScreenNode => ({
  class: class_,
  file: `lib/${class_}.dart`,
  line: 1,
  children: [],
  route,
  previewRoute,
});

describe("routes", () => {
  it("matches patterns with parameters and ignores query strings and trailing slashes", () => {
    expect(routeMatches("/item/:id", "/item/7")).toBe(true);
    expect(routeMatches("/item/:id", "/item/7/")).toBe(true);
    expect(routeMatches("/item/:id", "/item/7?x=1")).toBe(true);
    expect(routeMatches("/item/:id", "/item")).toBe(false);
    expect(routeMatches("/a/:x/b", "/a/1/c")).toBe(false);
    expect(routeMatches("/", "/")).toBe(true);
    expect(routeMatches("/", "/settings")).toBe(false);
  });

  it("finds the page for a route: exact example first, then a pattern; pages without a route never match", () => {
    const nodes = [n("Home", "/"), n("Settings", "/settings"), n("Detail", "/item/:id", "/item/1"), n("Orphan", null)];
    expect(pageForRoute(nodes, "/")?.class).toBe("Home");
    expect(pageForRoute(nodes, "/item/1")?.class).toBe("Detail");
    expect(pageForRoute(nodes, "/item/99")?.class).toBe("Detail");
    expect(pageForRoute(nodes, "/nowhere")).toBeNull();
  });
});
