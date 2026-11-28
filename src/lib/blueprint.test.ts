import { describe, expect, it } from "vitest";
import { addPage, movePage, removePage, renamePage, setNavigation, setSignIn, setSync, slug, type Blueprint } from "./blueprint";

const base = (): Blueprint => ({
  version: 1,
  app: { name: "Scores", purpose: "p", audience: "a", language: "en", description: "d" },
  style: { card: "calm", seed: "#0D9488", font: "Inter", kit: "rounded", logo: null },
  navigation: { kind: "tabs", order: ["home", "players"] },
  pages: [
    { id: "home", name: "Home", purpose: "", route: "/", components: [], models: [], features: ["f1"], links: ["players"] },
    { id: "players", name: "Players", purpose: "", route: "/players", components: [], models: [], features: [], links: [] },
  ],
  models: [],
  features: [{ id: "f1", title: "List", capability: "lists", pages: ["home", "players"], status: "now" }],
  integrations: { storage: "local", auth: [], purchases: false, notifications: false },
  assumptions: [],
});

describe("blueprint editing keeps the plan consistent", () => {
  it("slugs are safe and ids/routes stay unique when pages are added", () => {
    expect(slug("Wedstrijd Geschiedenis!")).toBe("wedstrijd_geschiedenis");
    expect(slug("123")).toBe("page");
    let b = addPage(base(), "Players");
    expect(b.pages.map((p) => p.id)).toEqual(["home", "players", "players_2"]);
    expect(b.pages[2]!.route).toBe("/players_2");
    expect(b.navigation.order).toContain("players_2");
    b = addPage(b, "   ");
    expect(b.pages).toHaveLength(3);
  });

  it("removing a page removes every reference; the start page and the last page cannot go", () => {
    const b = removePage(base(), "players");
    expect(b.pages.map((p) => p.id)).toEqual(["home"]);
    expect(b.pages[0]!.links).toEqual([]);
    expect(b.navigation.order).toEqual(["home"]);
    expect(b.features[0]!.pages).toEqual(["home"]);
    expect(removePage(b, "home")).toBe(b);
    expect(removePage(base(), "home").pages).toHaveLength(2);
  });

  it("moving and renaming work on the list and the navigation", () => {
    const b = movePage(base(), "players", -1);
    expect(b.pages.map((p) => p.id)).toEqual(["players", "home"]);
    expect(b.navigation.order).toEqual(["players", "home"]);
    expect(movePage(base(), "home", -1).pages.map((p) => p.id)).toEqual(["home", "players"]);
    expect(renamePage(base(), "home", "Scoreboard").pages[0]!.name).toBe("Scoreboard");
    expect(setNavigation(base(), "menu").navigation).toEqual({ kind: "menu", order: ["home", "players"] });
  });

  it("switching sign-in or sync on adds the matching feature and off removes it again", () => {
    let b = setSignIn(base(), true);
    expect(b.integrations.auth).toEqual(["email"]);
    expect(b.features.some((f) => f.capability === "auth_email" && f.status === "now")).toBe(true);
    b = setSignIn(b, false);
    expect(b.integrations.auth).toEqual([]);
    expect(b.features.some((f) => f.capability === "auth_email")).toBe(false);
    b = setSync(base(), "supabase");
    expect(b.integrations.storage).toBe("supabase");
    expect(b.features.some((f) => f.capability === "cloud_sync_supabase")).toBe(true);
    expect(setSync(b, null).integrations.storage).toBe("local");
  });
});
