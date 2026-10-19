// A component's template as the lowerer reads it: Svelte's modern AST, each
// fragment cleaned and flagged the way Svelte's server compiler cleans and
// flags it before it places a marker. Ported from
// `src/compiler/phases/3-transform/utils.js` (`clean_nodes`, `infer_namespace`)
// and the binding kinds of `src/compiler/phases/scope.js` at v5.57.2.
(function () {
  const SVG = new Set("altGlyph,altGlyphDef,altGlyphItem,animate,animateColor,animateMotion,animateTransform,circle,clipPath,color-profile,cursor,defs,desc,discard,ellipse,feBlend,feColorMatrix,feComponentTransfer,feComposite,feConvolveMatrix,feDiffuseLighting,feDisplacementMap,feDistantLight,feDropShadow,feFlood,feFuncA,feFuncB,feFuncG,feFuncR,feGaussianBlur,feImage,feMerge,feMergeNode,feMorphology,feOffset,fePointLight,feSpecularLighting,feSpotLight,feTile,feTurbulence,filter,font,font-face,font-face-format,font-face-name,font-face-src,font-face-uri,foreignObject,g,glyph,glyphRef,hatch,hatchpath,hkern,image,line,linearGradient,marker,mask,mesh,meshgradient,meshpatch,meshrow,metadata,missing-glyph,mpath,path,pattern,polygon,polyline,radialGradient,rect,set,solidcolor,stop,svg,switch,symbol,text,textPath,tref,tspan,unknown,use,view,vkern".split(","));
  const MATHML = new Set("annotation,annotation-xml,maction,math,merror,mfrac,mi,mmultiscripts,mn,mo,mover,mpadded,mphantom,mprescripts,mroot,mrow,ms,mspace,msqrt,mstyle,msub,msubsup,msup,mtable,mtd,mtext,mtr,munder,munderover,semantics".split(","));
  const HOISTED = new Set(["ConstTag", "DeclarationTag", "DebugTag", "SvelteBody", "SvelteWindow", "SvelteDocument", "SvelteHead", "TitleElement", "SnippetBlock"]);
  const TEXT_FIRST_PARENTS = new Set(["Fragment", "SnippetBlock", "EachBlock", "SvelteComponent", "SvelteBoundary", "Component", "SvelteSelf"]);
  const SLOTTABLE = new Set(["RegularElement", "SlotElement", "SvelteBody", "SvelteComponent", "SvelteDocument", "SvelteElement", "SvelteFragment", "SvelteHead", "SvelteOptions", "SvelteSelf", "SvelteWindow", "SvelteBoundary", "Component", "TitleElement"]);
  const REMOVABLE_SPACE_PARENTS = new Set(["select", "tr", "table", "tbody", "thead", "tfoot", "colgroup", "datalist"]);
  const STARTS_WS = /^[ \t\r\n]+/;
  const ENDS_WS = /[ \t\r\n]+$/;
  const NOT_WS = /[^ \t\r\n]/;

  /** Marks each element `svg` and `mathml` as Svelte's analysis does, an `a` or `title` taking its nearest element's. */
  function mark(nodes, parent_svg, top) {
    for (const node of nodes) {
      if (node.type === "RegularElement") {
        let svg = SVG.has(node.name) || ((node.name === "a" || node.name === "title") && parent_svg);
        if (node.name === "a" && top) {
          svg = svg || node.fragment.nodes.some((child) => child.type === "RegularElement" && SVG.has(child.name) && child.name !== "svg");
        }
        node.__svg = svg;
        node.__mathml = MATHML.has(node.name);
        mark(node.fragment.nodes, svg && node.name !== "foreignObject", false);
      } else {
        for (const fragment of fragments_of(node)) mark(fragment.nodes, parent_svg, top);
      }
    }
  }

  function fragments_of(node) {
    return [node.fragment, node.consequent, node.alternate, node.body, node.fallback, node.pending, node.then, node.catch].filter((f) => f && Array.isArray(f.nodes));
  }

  function namespace_check(nodes) {
    let namespace = "keep";
    const visit = (node) => {
      if (namespace === "html") return;
      if (node.type === "RegularElement" || node.type === "SvelteElement") {
        if (!node.__svg && !node.__mathml) {
          namespace = "html";
          return;
        }
        if (namespace === "keep") namespace = node.__svg ? "svg" : "mathml";
        node.fragment.nodes.forEach(visit);
      } else if (node.type === "Text") {
        if (node.data.trim() !== "") namespace = "maybe_html";
      } else if (["EachBlock", "IfBlock", "AwaitBlock", "KeyBlock"].includes(node.type)) {
        for (const fragment of fragments_of(node)) fragment.nodes.forEach(visit);
      }
    };
    for (const node of nodes) {
      visit(node);
      if (namespace === "html") return namespace;
    }
    return namespace;
  }

  /** `infer_namespace`. */
  function infer_namespace(namespace, parent, nodes) {
    if (parent.type === "RegularElement" && parent.name === "foreignObject") return "html";
    if (parent.type === "RegularElement" || parent.type === "SvelteElement") return parent.__svg ? "svg" : parent.__mathml ? "mathml" : "html";
    if (["Fragment", "Root", "Component", "SvelteComponent", "SvelteFragment", "SnippetBlock", "SlotElement"].includes(parent.type)) {
      const found = namespace_check(nodes);
      if (found !== "keep" && found !== "maybe_html") return found;
    }
    let found = null;
    for (const node of nodes) {
      if (node.type !== "RegularElement") continue;
      if (node.__mathml) found = found === null || found === "mathml" ? "mathml" : "html";
      else if (node.__svg) found = found === null || found === "svg" ? "svg" : "html";
      else return "html";
    }
    return found ?? namespace;
  }

  /** `clean_nodes`: the hoisted nodes, the rest with whitespace trimmed and joined, then the two flags. */
  function clean(parent, nodes, path, namespace, preserve) {
    const hoisted = [];
    let regular = [];
    for (const node of nodes) {
      if (node.type === "Comment") continue;
      if (HOISTED.has(node.type)) hoisted.push(node);
      else regular.push({ ...node });
    }
    let trimmed = regular;
    if (!preserve) {
      trimmed = [];
      while (regular[0] && regular[0].type === "Text" && !NOT_WS.test(regular[0].data)) regular.shift();
      if (regular[0]?.type === "Text") regular[0].data = regular[0].data.replace(STARTS_WS, "");
      while (regular.at(-1) && regular.at(-1).type === "Text" && !NOT_WS.test(regular.at(-1).data)) regular.pop();
      if (regular.at(-1)?.type === "Text") regular.at(-1).data = regular.at(-1).data.replace(ENDS_WS, "");
      const removable =
        (namespace === "svg" && (parent.type !== "RegularElement" || parent.name !== "text") && !path.some((n) => n.type === "RegularElement" && n.name === "text")) ||
        (parent.type === "RegularElement" && REMOVABLE_SPACE_PARENTS.has(parent.name));
      for (let i = 0; i < regular.length; i++) {
        const prev = regular[i - 1];
        const node = regular[i];
        const next = regular[i + 1];
        if (node.type === "Text") {
          if (prev?.type !== "ExpressionTag") {
            const prev_ends_ws = prev?.type === "Text" && ENDS_WS.test(prev.data);
            node.data = node.data.replace(STARTS_WS, prev_ends_ws ? "" : " ");
          }
          if (next?.type !== "ExpressionTag") node.data = node.data.replace(ENDS_WS, " ");
          if (node.data && (node.data !== " " || !removable)) trimmed.push(node);
        } else {
          trimmed.push(node);
        }
      }
    }
    let first = trimmed[0];
    if (parent.type === "RegularElement" && parent.name === "pre" && first?.type === "Text" && (first.data === "\n" || first.data === "\r\n")) {
      trimmed.shift();
      first = trimmed[0];
    }
    if (trimmed.length === 1 && first.type === "RegularElement" && first.name === "script") {
      trimmed.push({ type: "Comment", data: "", start: -1, end: -1 });
    }
    const standalone =
      parent.type !== "RegularElement" &&
      parent.type !== "SvelteElement" &&
      trimmed.length === 1 &&
      ((first.type === "RenderTag" && !first.__dynamic) || (first.type === "Component" && !first.__dynamic && !first.attributes.some((a) => a.type === "Attribute" && a.name.startsWith("--"))));
    const text_first = TEXT_FIRST_PARENTS.has(parent.type) && !!first && (first.type === "Text" || first.type === "ExpressionTag");
    return { hoisted, trimmed, standalone, text_first };
  }

  /** The names a pattern binds. */
  function names(pattern, out) {
    if (!pattern) return out;
    switch (pattern.type) {
      case "Identifier":
        out.push(pattern.name);
        break;
      case "ObjectPattern":
        for (const p of pattern.properties) names(p.type === "RestElement" ? p.argument : p.value, out);
        break;
      case "ArrayPattern":
        for (const e of pattern.elements) names(e, out);
        break;
      case "AssignmentPattern":
        names(pattern.left, out);
        break;
      case "RestElement":
        names(pattern.argument, out);
        break;
    }
    return out;
  }

  function rune(init) {
    let callee = init && init.type === "CallExpression" ? init.callee : null;
    if (!callee) return null;
    if (callee.type === "MemberExpression" && callee.object.type === "Identifier" && callee.property.type === "Identifier") return callee.object.name + "." + callee.property.name;
    return callee.type === "Identifier" ? callee.name : null;
  }

  /** Each name a script binds at its top with the kind `scope.js` gives it, of which only `normal` matters here. */
  function script_kinds(program, runes, out) {
    if (!program) return out;
    const assigned = new Set();
    if (!runes) {
      const walk = (node) => {
        if (!node || typeof node.type !== "string") return;
        if (node.type === "AssignmentExpression") names(node.left, []).forEach((n) => assigned.add(n));
        if (node.type === "UpdateExpression" && node.argument.type === "Identifier") assigned.add(node.argument.name);
        for (const key of Object.keys(node)) {
          const value = node[key];
          if (key === "parent" || key === "metadata") continue;
          if (Array.isArray(value)) value.forEach(walk);
          else if (value && typeof value === "object" && typeof value.type === "string") walk(value);
        }
      };
      walk(program);
    }
    for (const statement of program.body) {
      if (statement.type === "ImportDeclaration") {
        for (const s of statement.specifiers) out.set(s.local.name, "normal");
        continue;
      }
      const exported = statement.type === "ExportNamedDeclaration" && statement.declaration;
      const declaration = exported ? statement.declaration : statement;
      if (declaration.type === "FunctionDeclaration" || declaration.type === "ClassDeclaration") {
        if (declaration.id) out.set(declaration.id.name, "normal");
      } else if (declaration.type === "VariableDeclaration") {
        for (const d of declaration.declarations) {
          const called = rune(d.init);
          let kind = "normal";
          if (runes && called === "$props") kind = "prop";
          else if (runes && (called === "$state" || called === "$state.raw")) kind = "state";
          else if (runes && (called === "$derived" || called === "$derived.by")) kind = "derived";
          else if (!runes && exported && declaration.kind !== "const") kind = "prop";
          for (const name of names(d.id, [])) out.set(name, !runes && kind === "normal" && assigned.has(name) ? "state" : kind);
        }
      }
    }
    return out;
  }

  function locator(source) {
    const starts = [0];
    for (let i = 0; i < source.length; i++) if (source[i] === "\n") starts.push(i + 1);
    return (offset) => {
      let lo = 0;
      let hi = starts.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (starts[mid] <= offset) lo = mid;
        else hi = mid - 1;
      }
      return [lo + 1, offset - starts[lo] + 1];
    };
  }

  function byte_offset(source, offset) {
    let bytes = 0;
    for (let i = 0; i < offset; i++) {
      const code = source.charCodeAt(i);
      if (code < 0x80) bytes += 1;
      else if (code < 0x800) bytes += 2;
      else if (code >= 0xd800 && code < 0xdc00) {
        bytes += 4;
        i++;
      } else bytes += 3;
    }
    return bytes;
  }

  /**
   * The template as `{ fragment }`, every fragment `{ hoisted, nodes,
   * standalone, text_first }` and every expression its source with where it
   * starts, `{ code, at: [line, column] }`.
   */
  globalThis.__svelte_template = function (root, source, runes) {
    const at = locator(source);
    const code = (node) => (node ? { code: source.slice(node.start, node.end), at: at(node.start) } : null);
    const kinds = script_kinds(root.module && root.module.content, true, new Map());
    script_kinds(root.instance && root.instance.content, runes, kinds);
    const scopes = [kinds];
    const kind_of = (name) => {
      for (let i = scopes.length - 1; i >= 0; i--) if (scopes[i].has(name)) return scopes[i].get(name);
      return undefined;
    };
    const with_scope = (bindings, fn) => {
      scopes.push(bindings);
      try {
        return fn();
      } finally {
        scopes.pop();
      }
    };
    const snippets = (nodes) => new Map(nodes.filter((n) => n.type === "SnippetBlock").map((n) => [n.expression.name, "normal"]));

    function flag(nodes) {
      for (const node of nodes) {
        if (node.type === "Component") {
          const name = node.name.includes(".") ? node.name.slice(0, node.name.indexOf(".")) : node.name;
          const kind = kind_of(name);
          node.__dynamic = runes && kind !== undefined && (kind !== "normal" || node.name.includes("."));
        } else if (node.type === "RenderTag") {
          const call = node.expression.type === "ChainExpression" ? node.expression.expression : node.expression;
          node.__dynamic = call.callee.type !== "Identifier" || kind_of(call.callee.name) !== "normal";
        }
      }
    }

    function attribute(a) {
      switch (a.type) {
        case "Attribute":
          return { type: "attr", name: a.name, value: a.value === true ? true : (Array.isArray(a.value) ? a.value : [a.value]).map((part) => (part.type === "Text" ? { text: part.data } : code(part.expression))), at: at(a.start) };
        case "SpreadAttribute":
          return { type: "spread", ...code(a.expression) };
        case "AttachTag":
          return { type: "attach", ...code(a.expression) };
        case "StyleDirective":
          return { type: "directive", kind: "style", name: a.name, modifiers: a.modifiers, value: a.value === true ? true : (Array.isArray(a.value) ? a.value : [a.value]).map((part) => (part.type === "Text" ? { text: part.data } : code(part.expression))), at: at(a.start) };
        default:
          return { type: "directive", kind: a.type.replace(/Directive$/, "").toLowerCase(), name: a.name, modifiers: a.modifiers || [], expression: code(a.expression), at: at(a.start) };
      }
    }

    function fragment(parent, nodes, path, namespace, preserve, bindings) {
      return with_scope(bindings || snippets(nodes), () => {
        const ns = infer_namespace(namespace, parent, nodes);
        flag(nodes);
        const cleaned = clean(parent, nodes, path, ns, preserve);
        const inner = [...path, parent];
        return {
          hoisted: cleaned.hoisted.map((n) => node(n, inner, ns, preserve)),
          nodes: cleaned.trimmed.map((n) => node(n, inner, ns, preserve)),
          standalone: cleaned.standalone,
          text_first: cleaned.text_first,
        };
      });
    }

    /** `determine_slot`: the static `slot` attribute of an element-like node. */
    function slot_of(n) {
      if (!SLOTTABLE.has(n.type)) return null;
      for (const a of n.attributes) {
        if (a.type === "Attribute" && a.name === "slot" && Array.isArray(a.value) && a.value.length === 1 && a.value[0].type === "Text") return a.value[0].data;
      }
      return null;
    }

    /** A component's children as `build_inline_component` groups them: by slot, snippets aside, each group cleaned as its own fragment. */
    function slot_groups(n, path, namespace, preserve) {
      const groups = new Map();
      for (const child of n.fragment.nodes) {
        if (child.type === "SnippetBlock") continue;
        const name = slot_of(child) ?? "default";
        if (!groups.has(name)) groups.set(name, []);
        groups.get(name).push(child);
      }
      return with_scope(snippets(n.fragment.nodes), () => [...groups].map(([name, nodes]) => ({ name, fragment: fragment(n, nodes, path, namespace, preserve) })));
    }

    function node(n, path, namespace, preserve) {
      const where = at(n.start);
      switch (n.type) {
        case "Text":
          return { type: "text", data: n.data };
        case "Comment":
          return { type: "comment", data: n.data };
        case "ExpressionTag":
          return { type: "expr", ...code(n.expression) };
        case "HtmlTag":
          return { type: "html", ...code(n.expression) };
        case "ConstTag": {
          const d = n.declaration.declarations[0];
          return { type: "const", id: code(d.id), init: code(d.init), at: where };
        }
        case "RenderTag": {
          const call = n.expression.type === "ChainExpression" ? n.expression.expression : n.expression;
          return { type: "render", callee: code(call.callee), args: call.arguments.map(code), optional: n.expression.type === "ChainExpression" || !!call.optional, dynamic: n.__dynamic, at: where };
        }
        case "RegularElement": {
          const ns = n.name === "foreignObject" ? "html" : n.__svg ? "svg" : n.__mathml ? "mathml" : "html";
          const keep = preserve || n.name === "pre" || n.name === "textarea";
          return { type: "element", name: namespace === "html" ? n.name.toLowerCase() : n.name, svg: n.__svg, mathml: n.__mathml, attrs: n.attributes.map(attribute), fragment: fragment(n, n.fragment.nodes, path, ns, keep), at: where };
        }
        case "Component":
          return { type: "component", name: n.name, dynamic: n.__dynamic, attrs: n.attributes.map(attribute), fragment: fragment(n, n.fragment.nodes, path, namespace, preserve), slots: slot_groups(n, path, namespace, preserve), at: where };
        case "SlotElement":
          return { type: "slot", attrs: n.attributes.map(attribute), fragment: fragment(n, n.fragment.nodes, path, namespace, preserve), at: where };
        case "IfBlock":
          return { type: "if", elseif: n.elseif, test: code(n.test), consequent: fragment(n, n.consequent.nodes, path, namespace, preserve), alternate: n.alternate ? fragment(n, n.alternate.nodes, path, namespace, preserve) : null, at: where };
        case "EachBlock": {
          const bound = new Map(names(n.context, []).map((name) => [name, "each"]));
          if (n.index) bound.set(n.index, "each");
          const body = with_scope(bound, () => fragment(n, n.body.nodes, path, namespace, preserve));
          return { type: "each", expression: code(n.expression), context: code(n.context), index: n.index || null, key: code(n.key), body, fallback: n.fallback ? fragment(n, n.fallback.nodes, path, namespace, preserve) : null, at: where };
        }
        case "SnippetBlock": {
          const params = new Map(n.parameters.flatMap((p) => names(p, [])).map((name) => [name, "normal"]));
          const body = with_scope(params, () => fragment(n, n.body.nodes, path, namespace, preserve));
          return { type: "snippet", name: n.expression.name, params: n.parameters.map(code), body, at: where };
        }
        case "KeyBlock":
          return { type: "key", expression: code(n.expression), fragment: fragment(n, n.fragment.nodes, path, namespace, preserve), at: where };
        case "AwaitBlock": {
          const part = (f, pattern) => (f ? with_scope(new Map(names(pattern, []).map((name) => [name, "each"])), () => fragment(n, f.nodes, path, namespace, preserve)) : null);
          return { type: "await", expression: code(n.expression), value: code(n.value), error: code(n.error), pending: part(n.pending, null), then: part(n.then, n.value), catch: part(n.catch, n.error), at: where };
        }
        case "SvelteFragment":
        case "SvelteHead":
        case "SvelteBoundary":
        case "SvelteBody":
        case "SvelteWindow":
        case "SvelteDocument":
        case "TitleElement":
        case "SvelteSelf":
        case "SvelteComponent":
        case "SvelteElement":
          return { type: "special", kind: n.type, name: n.name, attrs: n.attributes.map(attribute), this: code(n.type === "SvelteElement" ? n.tag : n.expression), fragment: fragment(n, n.fragment.nodes, path, namespace, preserve), slots: n.type === "SvelteComponent" || n.type === "SvelteSelf" ? slot_groups(n, path, namespace, preserve) : [], at: where };
        default:
          return { type: "other", kind: n.type, at: where };
      }
    }

    mark(root.fragment.nodes, false, true);
    return {
      runes,
      instance: root.instance ? [byte_offset(source, root.instance.content.start), byte_offset(source, root.instance.content.end)] : null,
      module: root.module ? [byte_offset(source, root.module.content.start), byte_offset(source, root.module.content.end)] : null,
      fragment: fragment(root.fragment, root.fragment.nodes, [], "html", false),
    };
  };
})();
