import type { Selection, SelectionTarget } from "./types";

export type Kind = "button" | "text" | "listItem" | "icon" | "image" | "input" | "switch" | "bar" | "other";

/** Grove indeling van Flutter-widgets in woorden die een gebruiker kent. */
export function kindOf(widget: string): Kind {
  if (/Button$/.test(widget) || widget === "InkWell" || widget === "GestureDetector") return "button";
  if (/^(Text|RichText|SelectableText)$/.test(widget)) return "text";
  if (/^(ListTile|SwitchListTile|CheckboxListTile|RadioListTile|ExpansionTile)$/.test(widget)) return "listItem";
  if (/^(Icon|ImageIcon)$/.test(widget)) return "icon";
  if (/^(Image|CircleAvatar)$/.test(widget)) return "image";
  if (/^(TextField|TextFormField|DropdownButton|Slider)$/.test(widget)) return "input";
  if (/^(Switch|Checkbox|Radio)$/.test(widget)) return "switch";
  if (/^(AppBar|BottomNavigationBar|NavigationBar|TabBar)$/.test(widget)) return "bar";
  return "other";
}

export interface Summary {
  /** `single` = één element; `many` = meerdere. */
  kind: "single" | "many";
  element: Kind;
  widget: string;
  text: string | null;
  count: number;
  screen: string | null;
}

export function summarize(sel: Selection): Summary {
  const first: SelectionTarget | undefined = sel.targets[0];
  const screen = sel.screen ? sel.screen.replace(/(Screen|Page)$/, "") || sel.screen : null;
  const trim = (s: string | null) => (s && s.length > 28 ? `${s.slice(0, 27)}…` : s);
  return {
    kind: sel.targets.length > 1 ? "many" : "single",
    element: first ? kindOf(first.widget) : "other",
    widget: first?.widget ?? "",
    text: trim(first?.text ?? null),
    count: sel.targets.length,
    screen,
  };
}

/** Het toegestane bereik voor de controle achteraf. */
export function rangesOf(sel: Selection): [string, number, number][] {
  return sel.targets.map((t) => [t.file, t.startLine, t.endLine]);
}
