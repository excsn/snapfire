import { advance, settle } from "./harness.js";

/** What a query compares an element's text with: the text itself, a pattern or a function of the text and the element. */
export type Matcher = string | RegExp | ((content: string, element: Element | null) => boolean);

export interface MatcherOptions {
  /** `false` matches a string case-insensitively anywhere in the text. Defaults to `true`, the whole text. */
  exact?: boolean;
  normalizer?: (text: string) => string;
  trim?: boolean;
  collapseWhitespace?: boolean;
}

export interface SelectorMatcherOptions extends MatcherOptions {
  /** Only elements matching this selector. */
  selector?: string;
  /** Elements matching this selector are never matched; `false` ignores nothing. Defaults to `"script, style"`. */
  ignore?: string | false;
}

export interface ByRoleOptions {
  name?: Matcher;
  description?: Matcher;
  /** Includes elements hidden from the accessibility tree. */
  hidden?: boolean;
  level?: number;
  checked?: boolean;
  selected?: boolean;
  pressed?: boolean;
  expanded?: boolean;
  current?: boolean | string;
  busy?: boolean;
  /** Matches any role an element's `role` attribute lists rather than the first. */
  queryFallbacks?: boolean;
}

export interface WaitForOptions {
  /** How long, on the harness clock, before the wait gives up. Defaults to `configure`'s `asyncUtilTimeout`, 1000. */
  timeout?: number;
  /** How far the clock moves between tries. Defaults to 50. */
  interval?: number;
  onTimeout?: (error: Error) => Error;
}

const config = { testIdAttribute: "data-testid", asyncUtilTimeout: 1000 };

/** Sets the attribute `ByTestId` reads and the default wait timeout. */
export function configure(next: Partial<typeof config>): void {
  Object.assign(config, next);
}

export class TestingLibraryElementError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TestingLibraryElementError";
  }
}

export function getDefaultNormalizer(options: { trim?: boolean; collapseWhitespace?: boolean } = {}): (text: string) => string {
  const trim = options.trim ?? true;
  const collapse = options.collapseWhitespace ?? true;
  return (text) => {
    let out = text;
    if (collapse) out = out.replace(/\s+/g, " ");
    if (trim) out = out.trim();
    return out;
  };
}

function matches(text: string | null, node: Element | null, matcher: Matcher, options: MatcherOptions = {}): boolean {
  if (text === null) return false;
  const normalised = (options.normalizer ?? getDefaultNormalizer(options))(text);
  if (typeof matcher === "function") return matcher(normalised, node);
  if (matcher instanceof RegExp) {
    matcher.lastIndex = 0;
    return matcher.test(normalised);
  }
  const wanted = String(matcher);
  return options.exact === false ? normalised.toLowerCase().includes(wanted.toLowerCase()) : normalised === wanted;
}

function describeMatcher(matcher: Matcher): string {
  return typeof matcher === "function" ? "a function" : matcher instanceof RegExp ? String(matcher) : JSON.stringify(matcher);
}

/** The text an element holds itself: its text node children, joined. */
export function nodeText(el: Element): string {
  let out = "";
  for (const node of Array.from(el.childNodes)) {
    if (node.nodeType === 3) out += node.textContent ?? "";
  }
  return out;
}

function candidates(container: ParentNode, selector: string): Element[] {
  const own = typeof (container as Element).matches === "function" && (container as Element).matches(selector) ? [container as Element] : [];
  return [...own, ...Array.from(container.querySelectorAll(selector))];
}

function documentOf(node: ParentNode): Document {
  return (node as Node).nodeType === 9 ? (node as Document) : ((node as Node).ownerDocument ?? document);
}

function inlineStyle(el: Element): Record<string, string> {
  const out: Record<string, string> = {};
  for (const part of (el.getAttribute("style") ?? "").split(";")) {
    const at = part.indexOf(":");
    if (at !== -1) out[part.slice(0, at).trim().toLowerCase()] = part.slice(at + 1).trim();
  }
  return out;
}

/** Whether `el` is left out of the accessibility tree: hidden by an attribute, by `aria-hidden` or by an inline style, itself or through an ancestor. The runner lays nothing out, so a stylesheet's `display: none` is not seen. */
export function isInaccessible(el: Element): boolean {
  for (let at: Element | null = el; at; at = at.parentElement) {
    if (["SCRIPT", "STYLE", "TEMPLATE", "NOSCRIPT", "HEAD"].includes(at.tagName)) return true;
    if (at.hasAttribute("hidden") || at.getAttribute("aria-hidden") === "true") return true;
    const style = inlineStyle(at);
    if (style.display === "none" || style.visibility === "hidden") return true;
  }
  return el.tagName === "INPUT" && (el.getAttribute("type") ?? "").toLowerCase() === "hidden";
}

const LANDMARK_SCOPES = ["ARTICLE", "ASIDE", "MAIN", "NAV", "SECTION"];

function insideScope(el: Element): boolean {
  for (let up = el.parentElement; up; up = up.parentElement) if (LANDMARK_SCOPES.includes(up.tagName)) return true;
  return false;
}

function inputRole(el: Element): string | null {
  const type = (el.getAttribute("type") ?? "text").toLowerCase();
  const list = el.hasAttribute("list");
  switch (type) {
    case "button":
    case "image":
    case "reset":
    case "submit":
      return "button";
    case "checkbox":
      return "checkbox";
    case "radio":
      return "radio";
    case "range":
      return "slider";
    case "number":
      return "spinbutton";
    case "search":
      return list ? "combobox" : "searchbox";
    case "email":
    case "tel":
    case "text":
    case "url":
    case "":
      return list ? "combobox" : "textbox";
    default:
      return null;
  }
}

/** The role an element has with no `role` attribute, the HTML-AAM mapping for the elements a page renders. */
function implicitRole(el: Element): string | null {
  const tag = el.tagName.toLowerCase();
  switch (tag) {
    case "a":
    case "area":
      return el.hasAttribute("href") ? "link" : null;
    case "article":
      return "article";
    case "aside":
      return "complementary";
    case "blockquote":
      return "blockquote";
    case "button":
      return "button";
    case "caption":
      return "caption";
    case "code":
      return "code";
    case "datalist":
      return "listbox";
    case "dd":
      return "definition";
    case "del":
      return "deletion";
    case "details":
      return "group";
    case "dfn":
    case "dt":
      return "term";
    case "dialog":
      return "dialog";
    case "em":
      return "emphasis";
    case "fieldset":
      return "group";
    case "figure":
      return "figure";
    case "footer":
      return insideScope(el) ? null : "contentinfo";
    case "form":
      return "form";
    case "h1":
    case "h2":
    case "h3":
    case "h4":
    case "h5":
    case "h6":
      return "heading";
    case "header":
      return insideScope(el) ? null : "banner";
    case "hr":
      return "separator";
    case "html":
      return "document";
    case "img":
      return el.getAttribute("alt") === "" ? "presentation" : "img";
    case "input":
      return inputRole(el);
    case "ins":
      return "insertion";
    case "li":
      return "listitem";
    case "main":
      return "main";
    case "math":
      return "math";
    case "menu":
    case "ol":
    case "ul":
      return "list";
    case "meter":
      return "meter";
    case "nav":
      return "navigation";
    case "optgroup":
      return "group";
    case "option":
      return "option";
    case "output":
      return "status";
    case "p":
      return "paragraph";
    case "progress":
      return "progressbar";
    case "search":
      return "search";
    case "section":
      return el.hasAttribute("aria-label") || el.hasAttribute("aria-labelledby") || el.hasAttribute("title") ? "region" : null;
    case "select":
      return el.hasAttribute("multiple") || Number(el.getAttribute("size") ?? "0") > 1 ? "listbox" : "combobox";
    case "strong":
      return "strong";
    case "sub":
      return "subscript";
    case "sup":
      return "superscript";
    case "table":
      return "table";
    case "tbody":
    case "tfoot":
    case "thead":
      return "rowgroup";
    case "td": {
      const table = el.closest("table");
      const role = table?.getAttribute("role");
      return role === "grid" || role === "treegrid" ? "gridcell" : "cell";
    }
    case "textarea":
      return "textbox";
    case "th":
      return el.getAttribute("scope") === "row" ? "rowheader" : "columnheader";
    case "time":
      return "time";
    case "tr":
      return "row";
    default:
      return null;
  }
}

/** The roles an element answers to: the first token of its `role` attribute, every token with `fallbacks`, else the implicit role. */
export function rolesOf(el: Element, fallbacks = false): string[] {
  const explicit = (el.getAttribute("role") ?? "").trim().split(/\s+/).filter(Boolean);
  if (explicit.length > 0) return fallbacks ? explicit : [explicit[0]];
  const implicit = implicitRole(el);
  return implicit ? [implicit] : [];
}

const NAME_FROM_CONTENT = new Set(["button", "cell", "checkbox", "columnheader", "gridcell", "heading", "link", "menuitem", "menuitemcheckbox", "menuitemradio", "option", "radio", "row", "rowheader", "switch", "tab", "tooltip", "treeitem", "term", "caption", "legend"]);

const INLINE = new Set(["A", "ABBR", "B", "BDI", "BDO", "CITE", "CODE", "DATA", "DFN", "EM", "I", "KBD", "LABEL", "MARK", "Q", "S", "SAMP", "SMALL", "SPAN", "STRONG", "SUB", "SUP", "TIME", "U", "VAR", "IMG", "INPUT", "SELECT", "TEXTAREA", "BUTTON"]);

function labelsOf(el: Element): Element[] {
  const doc = el.ownerDocument;
  const out: Element[] = [];
  const id = el.getAttribute("id");
  if (id) out.push(...Array.from(doc.querySelectorAll("label")).filter((label) => label.getAttribute("for") === id));
  const wrapping = el.closest("label");
  if (wrapping && !out.includes(wrapping)) out.push(wrapping);
  return out;
}

/** A label's text without the controls inside it: `Quantity` for a label holding the word and a select. */
function labelText(label: Element): string {
  let out = "";
  for (const node of Array.from(label.childNodes)) {
    if (node.nodeType === 3) out += node.textContent ?? "";
    else if (node.nodeType === 1) {
      const el = node as Element;
      if (["SELECT", "TEXTAREA", "INPUT", "BUTTON"].includes(el.tagName)) continue;
      out += labelText(el);
    }
  }
  return out;
}

function contentText(el: Element, visited: Set<Element>): string {
  let out = "";
  for (const node of Array.from(el.childNodes)) {
    if (node.nodeType === 3) {
      out += node.textContent ?? "";
      continue;
    }
    if (node.nodeType !== 1) continue;
    const child = node as Element;
    if (isInaccessible(child)) continue;
    const gap = INLINE.has(child.tagName) ? "" : " ";
    out += gap + nameOf(child, visited, false) + gap;
  }
  return out;
}

function nativeName(el: Element, visited: Set<Element>): string {
  const tag = el.tagName;
  const type = (el.getAttribute("type") ?? "").toLowerCase();
  if (tag === "INPUT" && ["button", "submit", "reset"].includes(type)) return el.getAttribute("value") ?? (type === "submit" ? "Submit" : type === "reset" ? "Reset" : "");
  if (tag === "INPUT" && type === "image") return el.getAttribute("alt") ?? el.getAttribute("value") ?? "";
  if (["INPUT", "SELECT", "TEXTAREA", "METER", "PROGRESS", "OUTPUT", "BUTTON"].includes(tag)) {
    const labelled = labelsOf(el).map((label) => labelText(label)).join(" ");
    if (labelled.trim()) return labelled;
  }
  if (tag === "IMG" || tag === "AREA") return el.getAttribute("alt") ?? "";
  if (tag === "FIELDSET") {
    const legend = el.querySelector(":scope > legend");
    if (legend) return contentText(legend, visited);
  }
  if (tag === "FIGURE") {
    const caption = el.querySelector(":scope > figcaption");
    if (caption) return contentText(caption, visited);
  }
  if (tag === "TABLE") {
    const caption = el.querySelector(":scope > caption");
    if (caption) return contentText(caption, visited);
  }
  if (tag.toLowerCase() === "svg") {
    const title = el.querySelector(":scope > title");
    if (title) return title.textContent ?? "";
  }
  return "";
}

function nameOf(el: Element, visited: Set<Element>, root: boolean): string {
  if (visited.has(el)) return "";
  visited.add(el);
  const labelledby = el.getAttribute("aria-labelledby");
  if (root && labelledby) {
    const text = labelledby
      .split(/\s+/)
      .map((id) => el.ownerDocument.getElementById(id))
      .filter((target): target is HTMLElement => target !== null)
      .map((target) => nameOf(target, visited, false))
      .join(" ");
    if (text.trim()) return text;
  }
  const label = el.getAttribute("aria-label");
  if (label && label.trim()) return label;
  const native = nativeName(el, visited);
  if (native.trim()) return native;
  if (!root || rolesOf(el).some((role) => NAME_FROM_CONTENT.has(role))) {
    const text = contentText(el, visited);
    if (text.trim()) return text;
  }
  if (!root) return "";
  const title = el.getAttribute("title");
  if (title) return title;
  const placeholder = el.getAttribute("placeholder");
  return rolesOf(el).some((role) => role === "textbox" || role === "searchbox") && placeholder ? placeholder : "";
}

/** The element's accessible name, the way a screen reader would announce it: `aria-labelledby`, `aria-label`, its labels, its alt text, its content where its role takes a name from content, then its title. */
export function accessibleName(el: Element): string {
  return nameOf(el, new Set(), true).replace(/\s+/g, " ").trim();
}

/** The text `aria-describedby` points at, else the title when the title did not become the name. */
export function accessibleDescription(el: Element): string {
  const ids = el.getAttribute("aria-describedby");
  if (ids) {
    return ids
      .split(/\s+/)
      .map((id) => el.ownerDocument.getElementById(id)?.textContent ?? "")
      .join(" ")
      .replace(/\s+/g, " ")
      .trim();
  }
  const title = el.getAttribute("title") ?? "";
  return title && accessibleName(el) !== title ? title : "";
}

function levelOf(el: Element): number | null {
  const aria = el.getAttribute("aria-level");
  if (aria) return Number(aria);
  const match = /^H([1-6])$/.exec(el.tagName);
  return match ? Number(match[1]) : null;
}

function checkedOf(el: Element): boolean {
  const type = (el.getAttribute("type") ?? "").toLowerCase();
  if (el.tagName === "INPUT" && (type === "checkbox" || type === "radio")) return (el as HTMLInputElement).checked === true;
  return el.getAttribute("aria-checked") === "true";
}

function selectedOf(el: Element): boolean {
  if (el.tagName === "OPTION") return (el as HTMLOptionElement).selected === true;
  return el.getAttribute("aria-selected") === "true";
}

function currentOf(el: Element): boolean | string {
  const aria = el.getAttribute("aria-current");
  if (aria === null || aria === "false") return false;
  return aria === "true" ? true : aria;
}

type Finder<O> = (container: ParentNode, matcher: Matcher, options?: O) => Element[];

function inOrder(container: ParentNode, found: Set<Element>): Element[] {
  return candidates(container, "*").filter((el) => found.has(el));
}

const byText: Finder<SelectorMatcherOptions> = (container, matcher, options = {}) => {
  const ignore = options.ignore === undefined ? "script, style" : options.ignore;
  return candidates(container, options.selector ?? "*").filter((el) => (!ignore || !el.matches(ignore)) && matches(nodeText(el), el, matcher, options));
};

const byLabelText: Finder<SelectorMatcherOptions> = (container, matcher, options = {}) => {
  const found = new Set<Element>();
  const doc = documentOf(container);
  for (const label of candidates(container, "label")) {
    if (!matches(labelText(label), label, matcher, options) && !matches(label.textContent, label, matcher, options)) continue;
    const target = label.getAttribute("for");
    const control = target ? doc.getElementById(target) : label.querySelector("button, input:not([type=hidden]), meter, output, progress, select, textarea");
    if (control) found.add(control);
  }
  for (const el of candidates(container, "[aria-labelledby]")) {
    const text = (el.getAttribute("aria-labelledby") ?? "")
      .split(/\s+/)
      .map((id) => doc.getElementById(id)?.textContent ?? "")
      .join(" ");
    if (matches(text, el, matcher, options)) found.add(el);
  }
  for (const el of candidates(container, "[aria-label]")) {
    if (matches(el.getAttribute("aria-label"), el, matcher, options)) found.add(el);
  }
  const ordered = inOrder(container, found);
  return options.selector ? ordered.filter((el) => el.matches(options.selector as string)) : ordered;
};

const byAttribute =
  (selector: string, attribute: string): Finder<MatcherOptions> =>
  (container, matcher, options = {}) =>
    candidates(container, selector).filter((el) => matches(el.getAttribute(attribute), el, matcher, options));

const byPlaceholderText = byAttribute("[placeholder]", "placeholder");
const byAltText = byAttribute("img[alt], input[alt], area[alt]", "alt");

const byTitle: Finder<MatcherOptions> = (container, matcher, options = {}) =>
  candidates(container, "[title], svg > title").filter((el) => (el.tagName.toLowerCase() === "title" ? matches(el.textContent, el, matcher, options) : matches(el.getAttribute("title"), el, matcher, options)));

const byDisplayValue: Finder<MatcherOptions> = (container, matcher, options = {}) =>
  candidates(container, "input, select, textarea").filter((el) => {
    if (el.tagName === "SELECT") return Array.from(el.querySelectorAll("option")).some((o) => (o as HTMLOptionElement).selected && matches(o.textContent, el, matcher, options));
    const value = (el as HTMLInputElement).value;
    return matches(value === undefined ? el.getAttribute("value") : String(value), el, matcher, options);
  });

const byTestId: Finder<MatcherOptions> = (container, matcher, options = {}) => candidates(container, `[${config.testIdAttribute}]`).filter((el) => matches(el.getAttribute(config.testIdAttribute), el, matcher, options));

const byRole: Finder<ByRoleOptions> = (container, role, options = {}) =>
  candidates(container, "*").filter((el) => {
    if (!options.hidden && isInaccessible(el)) return false;
    if (!rolesOf(el, options.queryFallbacks).some((r) => (typeof role === "string" ? r === role : matches(r, el, role)))) return false;
    if (options.level !== undefined && levelOf(el) !== options.level) return false;
    if (options.checked !== undefined && checkedOf(el) !== options.checked) return false;
    if (options.selected !== undefined && selectedOf(el) !== options.selected) return false;
    if (options.pressed !== undefined && (el.getAttribute("aria-pressed") === "true") !== options.pressed) return false;
    if (options.expanded !== undefined && (el.getAttribute("aria-expanded") === "true") !== options.expanded) return false;
    if (options.current !== undefined && currentOf(el) !== options.current) return false;
    if (options.busy !== undefined && (el.getAttribute("aria-busy") === "true") !== options.busy) return false;
    if (options.name !== undefined && !matches(accessibleName(el), el, options.name)) return false;
    if (options.description !== undefined && !matches(accessibleDescription(el), el, options.description)) return false;
    return true;
  });

/** A line per accessible element under `container`, its role and name, for a failed role query to show what was there. */
function roleSummary(container: ParentNode): string {
  const lines: string[] = [];
  for (const el of candidates(container, "*")) {
    if (isInaccessible(el)) continue;
    const roles = rolesOf(el);
    if (roles.length === 0 || ["generic", "presentation", "document"].includes(roles[0])) continue;
    lines.push(`  ${roles[0]}: ${JSON.stringify(accessibleName(el))}`);
    if (lines.length === 40) {
      lines.push("  …");
      break;
    }
  }
  return lines.length > 0 ? `\n\nAccessible roles here:\n${lines.join("\n")}` : "\n\nThere are no accessible roles here.";
}

type Kind = "Role" | "Text" | "LabelText" | "PlaceholderText" | "AltText" | "Title" | "DisplayValue" | "TestId";

const FINDERS: Record<Kind, Finder<never>> = {
  Role: byRole as Finder<never>,
  Text: byText as Finder<never>,
  LabelText: byLabelText as Finder<never>,
  PlaceholderText: byPlaceholderText as Finder<never>,
  AltText: byAltText as Finder<never>,
  Title: byTitle as Finder<never>,
  DisplayValue: byDisplayValue as Finder<never>,
  TestId: byTestId as Finder<never>,
};

function what(kind: Kind, matcher: Matcher, options: object): string {
  const extra = Object.entries(options).filter(([, v]) => v !== undefined).map(([k, v]) => `${k}: ${v instanceof RegExp ? String(v) : JSON.stringify(v)}`);
  const described = kind === "Role" ? `with the role ${describeMatcher(matcher)}` : kind === "Text" ? `with the text ${describeMatcher(matcher)}` : `by ${kind} ${describeMatcher(matcher)}`;
  return extra.length > 0 ? `${described} (${extra.join(", ")})` : described;
}

function none(kind: Kind, container: ParentNode, matcher: Matcher, options: object): TestingLibraryElementError {
  return new TestingLibraryElementError(`Unable to find an element ${what(kind, matcher, options)}${kind === "Role" ? roleSummary(container) : ""}`);
}

function many(kind: Kind, found: Element[], matcher: Matcher, options: object): TestingLibraryElementError {
  return new TestingLibraryElementError(`Found ${found.length} elements ${what(kind, matcher, options)}: ${found.map((el) => `<${el.tagName.toLowerCase()}>`).join(", ")}. Use an All query when more than one is expected.`);
}

/** Resolves once `callback` stops throwing, retrying after everything due has run and then after moving the clock by `interval`, until `timeout` has passed on the harness clock. The clock moves only because this moves it, so timers in the page fire as the wait goes on. */
export async function waitFor<T>(callback: () => T | Promise<T>, options: WaitForOptions = {}): Promise<T> {
  const timeout = options.timeout ?? config.asyncUtilTimeout;
  const interval = options.interval ?? 50;
  let elapsed = 0;
  let last: unknown;
  for (;;) {
    try {
      return await callback();
    } catch (e) {
      last = e;
    }
    await settle();
    try {
      return await callback();
    } catch (e) {
      last = e;
    }
    if (elapsed >= timeout) {
      const error = last instanceof Error ? last : new Error(String(last));
      throw options.onTimeout ? options.onTimeout(error) : error;
    }
    await advance(interval);
    elapsed += interval;
  }
}

/** Resolves once the element, every element in the list or whatever `callback` returns is gone from the document. Refuses one that is not there to begin with. */
export async function waitForElementToBeRemoved(target: Element | Element[] | null | (() => Element | Element[] | null), options: WaitForOptions = {}): Promise<void> {
  const current = () => {
    const value = typeof target === "function" ? target() : target;
    return (Array.isArray(value) ? value : value ? [value] : []).filter((el) => el.isConnected);
  };
  if (current().length === 0) throw new TestingLibraryElementError("waitForElementToBeRemoved: the element is not in the document to begin with");
  await waitFor(() => {
    if (current().length > 0) throw new TestingLibraryElementError("waitForElementToBeRemoved: the element is still in the document");
  }, options);
}

type Options<K extends Kind> = K extends "Role" ? ByRoleOptions : K extends "Text" | "LabelText" ? SelectorMatcherOptions : MatcherOptions;

/** The eight queries of each kind: `getBy`, `getAllBy`, `queryBy`, `queryAllBy`, `findBy`, `findAllBy`. */
export interface BoundQueries {
  getByRole(role: Matcher, options?: ByRoleOptions): HTMLElement;
  getAllByRole(role: Matcher, options?: ByRoleOptions): HTMLElement[];
  queryByRole(role: Matcher, options?: ByRoleOptions): HTMLElement | null;
  queryAllByRole(role: Matcher, options?: ByRoleOptions): HTMLElement[];
  findByRole(role: Matcher, options?: ByRoleOptions, wait?: WaitForOptions): Promise<HTMLElement>;
  findAllByRole(role: Matcher, options?: ByRoleOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
  getByText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement;
  getAllByText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement[];
  queryByText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement | null;
  queryAllByText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement[];
  findByText(text: Matcher, options?: SelectorMatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
  findAllByText(text: Matcher, options?: SelectorMatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
  getByLabelText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement;
  getAllByLabelText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement[];
  queryByLabelText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement | null;
  queryAllByLabelText(text: Matcher, options?: SelectorMatcherOptions | ParentNode): HTMLElement[];
  findByLabelText(text: Matcher, options?: SelectorMatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
  findAllByLabelText(text: Matcher, options?: SelectorMatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
  getByPlaceholderText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement;
  getAllByPlaceholderText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
  queryByPlaceholderText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement | null;
  queryAllByPlaceholderText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
  findByPlaceholderText(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
  findAllByPlaceholderText(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
  getByAltText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement;
  getAllByAltText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
  queryByAltText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement | null;
  queryAllByAltText(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
  findByAltText(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
  findAllByAltText(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
  getByTitle(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement;
  getAllByTitle(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
  queryByTitle(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement | null;
  queryAllByTitle(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
  findByTitle(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
  findAllByTitle(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
  getByDisplayValue(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement;
  getAllByDisplayValue(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
  queryByDisplayValue(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement | null;
  queryAllByDisplayValue(text: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
  findByDisplayValue(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
  findAllByDisplayValue(text: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
  getByTestId(id: Matcher, options?: MatcherOptions | ParentNode): HTMLElement;
  getAllByTestId(id: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
  queryByTestId(id: Matcher, options?: MatcherOptions | ParentNode): HTMLElement | null;
  queryAllByTestId(id: Matcher, options?: MatcherOptions | ParentNode): HTMLElement[];
  findByTestId(id: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement>;
  findAllByTestId(id: Matcher, options?: MatcherOptions, wait?: WaitForOptions): Promise<HTMLElement[]>;
}

function isParent(value: unknown): value is ParentNode {
  return typeof value === "object" && value !== null && typeof (value as { querySelectorAll?: unknown }).querySelectorAll === "function";
}

/** The queries bound to `container`, read when a query runs, so `screen`'s follows the document each `load` installs. An options argument that is a node searches under it instead, which is how a spec named a root before queries took options. */
function bind(container: () => ParentNode): BoundQueries {
  const out: Record<string, unknown> = {};
  for (const kind of Object.keys(FINDERS) as Kind[]) {
    const find = FINDERS[kind] as Finder<Options<typeof kind>>;
    const all = (matcher: Matcher, options?: unknown): Element[] => {
      const root = isParent(options) ? options : container();
      const opts = isParent(options) ? {} : ((options ?? {}) as object);
      return find(root, matcher, opts as never);
    };
    const optionsOf = (options?: unknown): object => (isParent(options) ? {} : ((options ?? {}) as object));
    out[`queryAllBy${kind}`] = (matcher: Matcher, options?: unknown) => all(matcher, options);
    out[`queryBy${kind}`] = (matcher: Matcher, options?: unknown) => {
      const found = all(matcher, options);
      if (found.length > 1) throw many(kind, found, matcher, optionsOf(options));
      return found[0] ?? null;
    };
    out[`getAllBy${kind}`] = (matcher: Matcher, options?: unknown) => {
      const found = all(matcher, options);
      if (found.length === 0) throw none(kind, isParent(options) ? options : container(), matcher, optionsOf(options));
      return found;
    };
    out[`getBy${kind}`] = (matcher: Matcher, options?: unknown) => {
      const found = all(matcher, options);
      if (found.length === 0) throw none(kind, isParent(options) ? options : container(), matcher, optionsOf(options));
      if (found.length > 1) throw many(kind, found, matcher, optionsOf(options));
      return found[0];
    };
    out[`findAllBy${kind}`] = (matcher: Matcher, options?: unknown, wait?: WaitForOptions) => waitFor(() => (out[`getAllBy${kind}`] as (m: Matcher, o?: unknown) => Element[])(matcher, options), wait);
    out[`findBy${kind}`] = (matcher: Matcher, options?: unknown, wait?: WaitForOptions) => waitFor(() => (out[`getBy${kind}`] as (m: Matcher, o?: unknown) => Element)(matcher, options), wait);
  }
  return out as unknown as BoundQueries;
}

/** Markup for a failure message or a log: `node`'s outer HTML, the document's body by default, cut at `maxLength`. */
export function prettyDOM(node?: Element | Document | null, maxLength = 7000): string {
  const target = node ?? document.body;
  const html = (target as Document).nodeType === 9 ? ((target as Document).documentElement?.outerHTML ?? "") : (target as Element).outerHTML;
  return html.length > maxLength ? `${html.slice(0, maxLength)}…` : html;
}

/** Logs every accessible element under `container` with its role and name. */
export function logRoles(container: ParentNode = document.body): void {
  console.log(roleSummary(container).trim());
}

export interface Screen extends BoundQueries {
  debug(element?: Element | Element[] | null, maxLength?: number): void;
  logTestingPlaygroundURL(): void;
}

/** The queries over the document's body. */
export const screen: Screen = Object.assign(bind(() => document.body), {
  debug(element?: Element | Element[] | null, maxLength?: number) {
    const targets = Array.isArray(element) ? element : [element ?? document.body];
    for (const target of targets) console.log(prettyDOM(target, maxLength));
  },
  logTestingPlaygroundURL() {
    console.log("fsr test has no playground; screen.debug() prints the markup");
  },
});

/** The queries over `container` alone. */
export function within(container: ParentNode): BoundQueries {
  return bind(() => container);
}

/** The labelable control a label forwards a click to: the one its `for` names, else the first inside it. */
export function controlOf(label: Element): Element | null {
  const target = label.getAttribute("for");
  if (target) return label.ownerDocument.getElementById(target);
  return label.querySelector("button, input:not([type=hidden]), meter, output, progress, select, textarea");
}
