import { advance, settle } from "./harness.js";
const config = {
    testIdAttribute: "data-testid",
    asyncUtilTimeout: 1000
};
export function configure(next) {
    Object.assign(config, next);
}
export class TestingLibraryElementError extends Error {
    constructor(message){
        super(message);
        this.name = "TestingLibraryElementError";
    }
}
export function getDefaultNormalizer(options = {}) {
    const trim = options.trim ?? true;
    const collapse = options.collapseWhitespace ?? true;
    return (text)=>{
        let out = text;
        if (collapse) out = out.replace(/\s+/g, " ");
        if (trim) out = out.trim();
        return out;
    };
}
function matches(text, node, matcher, options = {}) {
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
function describeMatcher(matcher) {
    return typeof matcher === "function" ? "a function" : matcher instanceof RegExp ? String(matcher) : JSON.stringify(matcher);
}
export function nodeText(el) {
    let out = "";
    for (const node of Array.from(el.childNodes)){
        if (node.nodeType === 3) out += node.textContent ?? "";
    }
    return out;
}
function candidates(container, selector) {
    const own = typeof container.matches === "function" && container.matches(selector) ? [
        container
    ] : [];
    return [
        ...own,
        ...Array.from(container.querySelectorAll(selector))
    ];
}
function documentOf(node) {
    return node.nodeType === 9 ? node : node.ownerDocument ?? document;
}
function inlineStyle(el) {
    const out = {};
    for (const part of (el.getAttribute("style") ?? "").split(";")){
        const at = part.indexOf(":");
        if (at !== -1) out[part.slice(0, at).trim().toLowerCase()] = part.slice(at + 1).trim();
    }
    return out;
}
export function isInaccessible(el) {
    for(let at = el; at; at = at.parentElement){
        if ([
            "SCRIPT",
            "STYLE",
            "TEMPLATE",
            "NOSCRIPT",
            "HEAD"
        ].includes(at.tagName)) return true;
        if (at.hasAttribute("hidden") || at.getAttribute("aria-hidden") === "true") return true;
        const style = inlineStyle(at);
        if (style.display === "none" || style.visibility === "hidden") return true;
    }
    return el.tagName === "INPUT" && (el.getAttribute("type") ?? "").toLowerCase() === "hidden";
}
const LANDMARK_SCOPES = [
    "ARTICLE",
    "ASIDE",
    "MAIN",
    "NAV",
    "SECTION"
];
function insideScope(el) {
    for(let up = el.parentElement; up; up = up.parentElement)if (LANDMARK_SCOPES.includes(up.tagName)) return true;
    return false;
}
function inputRole(el) {
    const type = (el.getAttribute("type") ?? "text").toLowerCase();
    const list = el.hasAttribute("list");
    switch(type){
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
function implicitRole(el) {
    const tag = el.tagName.toLowerCase();
    switch(tag){
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
        case "td":
            {
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
export function rolesOf(el, fallbacks = false) {
    const explicit = (el.getAttribute("role") ?? "").trim().split(/\s+/).filter(Boolean);
    if (explicit.length > 0) return fallbacks ? explicit : [
        explicit[0]
    ];
    const implicit = implicitRole(el);
    return implicit ? [
        implicit
    ] : [];
}
const NAME_FROM_CONTENT = new Set([
    "button",
    "cell",
    "checkbox",
    "columnheader",
    "gridcell",
    "heading",
    "link",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "radio",
    "row",
    "rowheader",
    "switch",
    "tab",
    "tooltip",
    "treeitem",
    "term",
    "caption",
    "legend"
]);
const INLINE = new Set([
    "A",
    "ABBR",
    "B",
    "BDI",
    "BDO",
    "CITE",
    "CODE",
    "DATA",
    "DFN",
    "EM",
    "I",
    "KBD",
    "LABEL",
    "MARK",
    "Q",
    "S",
    "SAMP",
    "SMALL",
    "SPAN",
    "STRONG",
    "SUB",
    "SUP",
    "TIME",
    "U",
    "VAR",
    "IMG",
    "INPUT",
    "SELECT",
    "TEXTAREA",
    "BUTTON"
]);
function labelsOf(el) {
    const doc = el.ownerDocument;
    const out = [];
    const id = el.getAttribute("id");
    if (id) out.push(...Array.from(doc.querySelectorAll("label")).filter((label)=>label.getAttribute("for") === id));
    const wrapping = el.closest("label");
    if (wrapping && !out.includes(wrapping)) out.push(wrapping);
    return out;
}
function labelText(label) {
    let out = "";
    for (const node of Array.from(label.childNodes)){
        if (node.nodeType === 3) out += node.textContent ?? "";
        else if (node.nodeType === 1) {
            const el = node;
            if ([
                "SELECT",
                "TEXTAREA",
                "INPUT",
                "BUTTON"
            ].includes(el.tagName)) continue;
            out += labelText(el);
        }
    }
    return out;
}
function contentText(el, visited) {
    let out = "";
    for (const node of Array.from(el.childNodes)){
        if (node.nodeType === 3) {
            out += node.textContent ?? "";
            continue;
        }
        if (node.nodeType !== 1) continue;
        const child = node;
        if (isInaccessible(child)) continue;
        const gap = INLINE.has(child.tagName) ? "" : " ";
        out += gap + nameOf(child, visited, false) + gap;
    }
    return out;
}
function nativeName(el, visited) {
    const tag = el.tagName;
    const type = (el.getAttribute("type") ?? "").toLowerCase();
    if (tag === "INPUT" && [
        "button",
        "submit",
        "reset"
    ].includes(type)) return el.getAttribute("value") ?? (type === "submit" ? "Submit" : type === "reset" ? "Reset" : "");
    if (tag === "INPUT" && type === "image") return el.getAttribute("alt") ?? el.getAttribute("value") ?? "";
    if ([
        "INPUT",
        "SELECT",
        "TEXTAREA",
        "METER",
        "PROGRESS",
        "OUTPUT",
        "BUTTON"
    ].includes(tag)) {
        const labelled = labelsOf(el).map((label)=>labelText(label)).join(" ");
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
function nameOf(el, visited, root) {
    if (visited.has(el)) return "";
    visited.add(el);
    const labelledby = el.getAttribute("aria-labelledby");
    if (root && labelledby) {
        const text = labelledby.split(/\s+/).map((id)=>el.ownerDocument.getElementById(id)).filter((target)=>target !== null).map((target)=>nameOf(target, visited, false)).join(" ");
        if (text.trim()) return text;
    }
    const label = el.getAttribute("aria-label");
    if (label && label.trim()) return label;
    const native = nativeName(el, visited);
    if (native.trim()) return native;
    if (!root || rolesOf(el).some((role)=>NAME_FROM_CONTENT.has(role))) {
        const text = contentText(el, visited);
        if (text.trim()) return text;
    }
    if (!root) return "";
    const title = el.getAttribute("title");
    if (title) return title;
    const placeholder = el.getAttribute("placeholder");
    return rolesOf(el).some((role)=>role === "textbox" || role === "searchbox") && placeholder ? placeholder : "";
}
export function accessibleName(el) {
    return nameOf(el, new Set(), true).replace(/\s+/g, " ").trim();
}
export function accessibleDescription(el) {
    const ids = el.getAttribute("aria-describedby");
    if (ids) {
        return ids.split(/\s+/).map((id)=>el.ownerDocument.getElementById(id)?.textContent ?? "").join(" ").replace(/\s+/g, " ").trim();
    }
    const title = el.getAttribute("title") ?? "";
    return title && accessibleName(el) !== title ? title : "";
}
function levelOf(el) {
    const aria = el.getAttribute("aria-level");
    if (aria) return Number(aria);
    const match = /^H([1-6])$/.exec(el.tagName);
    return match ? Number(match[1]) : null;
}
function checkedOf(el) {
    const type = (el.getAttribute("type") ?? "").toLowerCase();
    if (el.tagName === "INPUT" && (type === "checkbox" || type === "radio")) return el.checked === true;
    return el.getAttribute("aria-checked") === "true";
}
function selectedOf(el) {
    if (el.tagName === "OPTION") return el.selected === true;
    return el.getAttribute("aria-selected") === "true";
}
function currentOf(el) {
    const aria = el.getAttribute("aria-current");
    if (aria === null || aria === "false") return false;
    return aria === "true" ? true : aria;
}
function inOrder(container, found) {
    return candidates(container, "*").filter((el)=>found.has(el));
}
const byText = (container, matcher, options = {})=>{
    const ignore = options.ignore === undefined ? "script, style" : options.ignore;
    return candidates(container, options.selector ?? "*").filter((el)=>(!ignore || !el.matches(ignore)) && matches(nodeText(el), el, matcher, options));
};
const byLabelText = (container, matcher, options = {})=>{
    const found = new Set();
    const doc = documentOf(container);
    for (const label of candidates(container, "label")){
        if (!matches(labelText(label), label, matcher, options) && !matches(label.textContent, label, matcher, options)) continue;
        const target = label.getAttribute("for");
        const control = target ? doc.getElementById(target) : label.querySelector("button, input:not([type=hidden]), meter, output, progress, select, textarea");
        if (control) found.add(control);
    }
    for (const el of candidates(container, "[aria-labelledby]")){
        const text = (el.getAttribute("aria-labelledby") ?? "").split(/\s+/).map((id)=>doc.getElementById(id)?.textContent ?? "").join(" ");
        if (matches(text, el, matcher, options)) found.add(el);
    }
    for (const el of candidates(container, "[aria-label]")){
        if (matches(el.getAttribute("aria-label"), el, matcher, options)) found.add(el);
    }
    const ordered = inOrder(container, found);
    return options.selector ? ordered.filter((el)=>el.matches(options.selector)) : ordered;
};
const byAttribute = (selector, attribute)=>(container, matcher, options = {})=>candidates(container, selector).filter((el)=>matches(el.getAttribute(attribute), el, matcher, options));
const byPlaceholderText = byAttribute("[placeholder]", "placeholder");
const byAltText = byAttribute("img[alt], input[alt], area[alt]", "alt");
const byTitle = (container, matcher, options = {})=>candidates(container, "[title], svg > title").filter((el)=>el.tagName.toLowerCase() === "title" ? matches(el.textContent, el, matcher, options) : matches(el.getAttribute("title"), el, matcher, options));
const byDisplayValue = (container, matcher, options = {})=>candidates(container, "input, select, textarea").filter((el)=>{
        if (el.tagName === "SELECT") return Array.from(el.querySelectorAll("option")).some((o)=>o.selected && matches(o.textContent, el, matcher, options));
        const value = el.value;
        return matches(value === undefined ? el.getAttribute("value") : String(value), el, matcher, options);
    });
const byTestId = (container, matcher, options = {})=>candidates(container, `[${config.testIdAttribute}]`).filter((el)=>matches(el.getAttribute(config.testIdAttribute), el, matcher, options));
const byRole = (container, role, options = {})=>candidates(container, "*").filter((el)=>{
        if (!options.hidden && isInaccessible(el)) return false;
        if (!rolesOf(el, options.queryFallbacks).some((r)=>typeof role === "string" ? r === role : matches(r, el, role))) return false;
        if (options.level !== undefined && levelOf(el) !== options.level) return false;
        if (options.checked !== undefined && checkedOf(el) !== options.checked) return false;
        if (options.selected !== undefined && selectedOf(el) !== options.selected) return false;
        if (options.pressed !== undefined && el.getAttribute("aria-pressed") === "true" !== options.pressed) return false;
        if (options.expanded !== undefined && el.getAttribute("aria-expanded") === "true" !== options.expanded) return false;
        if (options.current !== undefined && currentOf(el) !== options.current) return false;
        if (options.busy !== undefined && el.getAttribute("aria-busy") === "true" !== options.busy) return false;
        if (options.name !== undefined && !matches(accessibleName(el), el, options.name)) return false;
        if (options.description !== undefined && !matches(accessibleDescription(el), el, options.description)) return false;
        return true;
    });
function roleSummary(container) {
    const lines = [];
    for (const el of candidates(container, "*")){
        if (isInaccessible(el)) continue;
        const roles = rolesOf(el);
        if (roles.length === 0 || [
            "generic",
            "presentation",
            "document"
        ].includes(roles[0])) continue;
        lines.push(`  ${roles[0]}: ${JSON.stringify(accessibleName(el))}`);
        if (lines.length === 40) {
            lines.push("  …");
            break;
        }
    }
    return lines.length > 0 ? `\n\nAccessible roles here:\n${lines.join("\n")}` : "\n\nThere are no accessible roles here.";
}
const FINDERS = {
    Role: byRole,
    Text: byText,
    LabelText: byLabelText,
    PlaceholderText: byPlaceholderText,
    AltText: byAltText,
    Title: byTitle,
    DisplayValue: byDisplayValue,
    TestId: byTestId
};
function what(kind, matcher, options) {
    const extra = Object.entries(options).filter(([, v])=>v !== undefined).map(([k, v])=>`${k}: ${v instanceof RegExp ? String(v) : JSON.stringify(v)}`);
    const described = kind === "Role" ? `with the role ${describeMatcher(matcher)}` : kind === "Text" ? `with the text ${describeMatcher(matcher)}` : `by ${kind} ${describeMatcher(matcher)}`;
    return extra.length > 0 ? `${described} (${extra.join(", ")})` : described;
}
function none(kind, container, matcher, options) {
    return new TestingLibraryElementError(`Unable to find an element ${what(kind, matcher, options)}${kind === "Role" ? roleSummary(container) : ""}`);
}
function many(kind, found, matcher, options) {
    return new TestingLibraryElementError(`Found ${found.length} elements ${what(kind, matcher, options)}: ${found.map((el)=>`<${el.tagName.toLowerCase()}>`).join(", ")}. Use an All query when more than one is expected.`);
}
export async function waitFor(callback, options = {}) {
    const timeout = options.timeout ?? config.asyncUtilTimeout;
    const interval = options.interval ?? 50;
    let elapsed = 0;
    let last;
    for(;;){
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
export async function waitForElementToBeRemoved(target, options = {}) {
    const current = ()=>{
        const value = typeof target === "function" ? target() : target;
        return (Array.isArray(value) ? value : value ? [
            value
        ] : []).filter((el)=>el.isConnected);
    };
    if (current().length === 0) throw new TestingLibraryElementError("waitForElementToBeRemoved: the element is not in the document to begin with");
    await waitFor(()=>{
        if (current().length > 0) throw new TestingLibraryElementError("waitForElementToBeRemoved: the element is still in the document");
    }, options);
}
function isParent(value) {
    return typeof value === "object" && value !== null && typeof value.querySelectorAll === "function";
}
function bind(container) {
    const out = {};
    for (const kind of Object.keys(FINDERS)){
        const find = FINDERS[kind];
        const all = (matcher, options)=>{
            const root = isParent(options) ? options : container();
            const opts = isParent(options) ? {} : options ?? {};
            return find(root, matcher, opts);
        };
        const optionsOf = (options)=>isParent(options) ? {} : options ?? {};
        out[`queryAllBy${kind}`] = (matcher, options)=>all(matcher, options);
        out[`queryBy${kind}`] = (matcher, options)=>{
            const found = all(matcher, options);
            if (found.length > 1) throw many(kind, found, matcher, optionsOf(options));
            return found[0] ?? null;
        };
        out[`getAllBy${kind}`] = (matcher, options)=>{
            const found = all(matcher, options);
            if (found.length === 0) throw none(kind, isParent(options) ? options : container(), matcher, optionsOf(options));
            return found;
        };
        out[`getBy${kind}`] = (matcher, options)=>{
            const found = all(matcher, options);
            if (found.length === 0) throw none(kind, isParent(options) ? options : container(), matcher, optionsOf(options));
            if (found.length > 1) throw many(kind, found, matcher, optionsOf(options));
            return found[0];
        };
        out[`findAllBy${kind}`] = (matcher, options, wait)=>waitFor(()=>out[`getAllBy${kind}`](matcher, options), wait);
        out[`findBy${kind}`] = (matcher, options, wait)=>waitFor(()=>out[`getBy${kind}`](matcher, options), wait);
    }
    return out;
}
export function prettyDOM(node, maxLength = 7000) {
    const target = node ?? document.body;
    const html = target.nodeType === 9 ? target.documentElement?.outerHTML ?? "" : target.outerHTML;
    return html.length > maxLength ? `${html.slice(0, maxLength)}…` : html;
}
export function logRoles(container = document.body) {
    console.log(roleSummary(container).trim());
}
export const screen = Object.assign(bind(()=>document.body), {
    debug (element, maxLength) {
        const targets = Array.isArray(element) ? element : [
            element ?? document.body
        ];
        for (const target of targets)console.log(prettyDOM(target, maxLength));
    },
    logTestingPlaygroundURL () {
        console.log("fsr test has no playground; screen.debug() prints the markup");
    }
});
export function within(container) {
    return bind(()=>container);
}
export function controlOf(label) {
    const target = label.getAttribute("for");
    if (target) return label.ownerDocument.getElementById(target);
    return label.querySelector("button, input:not([type=hidden]), meter, output, progress, select, textarea");
}
//# sourceMappingURL=queries.js.map
