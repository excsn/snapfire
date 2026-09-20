import * as sfc from "@vue/compiler-sfc";

if (typeof globalThis.console === "undefined") {
  const quiet = function () {};
  globalThis.console = { log: quiet, warn: quiet, error: quiet, info: quiet, debug: quiet };
}

globalThis.__vue_version = function () {
  return sfc.version;
};

/** Stable per component and per content: the attribute a scoped style selects on. */
function scopeHash(filename, source) {
  let h = 0x811c9dc5;
  const text = filename + " " + source;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

function diagnostic(error, filename, severity) {
  const loc = error && error.loc && error.loc.start;
  return {
    severity: severity || "error",
    message: String((error && error.message) || error),
    file: filename,
    line: loc ? loc.line : undefined,
    column: loc ? loc.column : undefined,
  };
}

/** A block written in a language the plugin cannot hand on, named rather than emitted wrong. */
function unsupported(block, kind, filename) {
  const lang = block.lang;
  if (!lang) return null;
  if (kind === "script" && (lang === "ts" || lang === "js" || lang === "tsx" || lang === "jsx")) return null;
  if (kind === "style" && lang === "css") return null;
  return diagnostic({ message: kind + ' lang="' + lang + '" needs a compiler this plugin does not carry', loc: block.loc }, filename);
}

/** Gives a block with `src` the file's content, or names the file when the host has not sent it yet. */
function external(block, files, deps, needs) {
  if (!block || !block.src) return;
  deps.push(block.src);
  if (Object.prototype.hasOwnProperty.call(files, block.src)) {
    block.content = files[block.src];
  } else {
    needs.push(block.src);
  }
}

/**
 * Parses one component and settles what every answer shares. Comments are
 * left out of the template on both paths, so the module the browser runs
 * and the description the host lowers agree on the nodes.
 */
function open(filename, source, options, files) {
  const diagnostics = [];
  const deps = [];
  const needs = [];
  const parsed = sfc.parse(source, { filename, sourceMap: !!options.source_map, templateParseOptions: { comments: false } });
  if (parsed.errors.length) {
    return { failed: { status: "failed", diagnostics: parsed.errors.map((e) => diagnostic(e, filename)) } };
  }
  const descriptor = parsed.descriptor;
  const hash = scopeHash(filename, source);
  const scopeId = "data-v-" + hash;
  const hasScoped = descriptor.styles.some((s) => s.scoped);
  let lang = "js";
  for (const block of [descriptor.script, descriptor.scriptSetup].filter(Boolean)) {
    const refused = unsupported(block, "script", filename);
    if (refused) diagnostics.push(refused);
    external(block, files, deps, needs);
    if (block.lang === "ts" || block.lang === "tsx") lang = "ts";
  }
  for (const style of descriptor.styles) {
    const refused = unsupported(style, "style", filename);
    if (refused) diagnostics.push(refused);
    external(style, files, deps, needs);
  }
  external(descriptor.template, files, deps, needs);
  if (diagnostics.some((d) => d.severity === "error")) {
    return { failed: { status: "failed", diagnostics } };
  }
  if (needs.length) {
    return { failed: { status: "needs", files: needs } };
  }
  return { descriptor, hash, scopeId, hasScoped, lang, diagnostics, deps };
}

/** The script as `_sfc_main` plus the bindings the template resolves through. */
function script(opened) {
  const { descriptor, hash } = opened;
  if (!descriptor.script && !descriptor.scriptSetup) {
    return { code: "const _sfc_main = {};", bindings: undefined };
  }
  const compiled = sfc.compileScript(descriptor, {
    id: hash,
    isProd: !!opened.production,
    inlineTemplate: false,
    genDefaultAs: "_sfc_main",
  });
  return { code: compiled.content, bindings: compiled.bindings };
}

/**
 * The template as a render function, `ssr` picking the server renderer's.
 * The template is compiled from its source rather than from the parsed
 * tree: the compiler transforms the tree it is given in place and the parse
 * is cached by source, so a description read after a compile would see the
 * transformed tree.
 */
function template(opened, bindings, ssr) {
  const { descriptor, hash, scopeId, hasScoped } = opened;
  return sfc.compileTemplate({
    source: descriptor.template.content,
    filename: opened.filename,
    id: hash,
    scoped: hasScoped,
    slotted: descriptor.slotted,
    ssr,
    isProd: !!opened.production,
    compilerOptions: { bindingMetadata: bindings, scopeId: hasScoped ? scopeId : null, comments: false },
  });
}

/** A component as one module: the script, the render function bound as `fn`, the scope id and the file. */
function assemble(opened, filename, ssr) {
  const parts = [];
  const compiled = script(opened);
  parts.push(compiled.code);
  if (opened.descriptor.template) {
    const rendered = template(opened, compiled.bindings, ssr);
    if (rendered.errors.length) {
      return { failed: { status: "failed", diagnostics: rendered.errors.map((e) => diagnostic(e, filename)) } };
    }
    for (const tip of rendered.tips || []) opened.diagnostics.push(diagnostic({ message: tip }, filename, "warning"));
    parts.push(rendered.code);
    parts.push(ssr ? "_sfc_main.ssrRender = ssrRender;" : "_sfc_main.render = render;");
  }
  if (opened.hasScoped) parts.push("_sfc_main.__scopeId = " + JSON.stringify(opened.scopeId) + ";");
  parts.push("_sfc_main.__file = " + JSON.stringify(filename) + ";");
  parts.push("export default _sfc_main;");
  return { js: parts.join("\n") };
}

globalThis.__vue_compile = function (filename, source, options, files) {
  files = files || {};
  try {
    const opened = open(filename, source, options, files);
    if (opened.failed) return JSON.stringify(opened.failed);
    opened.filename = filename;
    opened.production = !!options.production;
    const assembled = assemble(opened, filename, false);
    if (assembled.failed) return JSON.stringify(assembled.failed);

    const styles = [];
    for (const style of opened.descriptor.styles) {
      const compiled = sfc.compileStyle({
        source: style.content,
        filename,
        id: opened.scopeId,
        scoped: !!style.scoped,
        trim: true,
      });
      if (compiled.errors.length) {
        return JSON.stringify({ status: "failed", diagnostics: compiled.errors.map((e) => diagnostic(e, filename)) });
      }
      styles.push(compiled.code);
    }

    return JSON.stringify({
      status: "ok",
      js: assembled.js,
      lang: opened.lang,
      css: styles.length ? styles.join("\n") : undefined,
      deps: opened.deps,
      diagnostics: opened.diagnostics,
    });
  } catch (e) {
    return JSON.stringify({ status: "failed", diagnostics: [diagnostic(e, filename)] });
  }
};

/**
 * The template tree the description carries, one object per node. Vue's
 * node types are numbered in `@vue/compiler-core`'s `NodeTypes`: element 1,
 * text 2, comment 3, interpolation 5, attribute 6, directive 7. An element's
 * `kind` is its `ElementTypes`: element 0, component 1, slot 2, template 3.
 * A node of any other type is passed through as `other` with its number, so
 * the host names it rather than skipping it. A prop's `line` and `column`
 * are its own; a directive's `expLine` and `expColumn` are its expression's.
 */
function describeNode(node) {
  const at = (loc) => (loc && loc.start ? { line: loc.start.line, column: loc.start.column } : { line: 0, column: 0 });
  switch (node.type) {
    case 1: {
      const props = node.props.map((prop) => {
        if (prop.type === 6) {
          return Object.assign({ prop: "attribute", name: prop.name, value: prop.value ? prop.value.content : null }, at(prop.loc));
        }
        const exp = prop.exp ? prop.exp.content : null;
        const where = at(prop.exp && prop.exp.loc ? prop.exp.loc : prop.loc);
        return Object.assign(
          {
            prop: "directive",
            name: prop.name,
            arg: prop.arg ? prop.arg.content : null,
            argStatic: prop.arg ? !!prop.arg.isStatic : true,
            exp,
            expLine: where.line,
            expColumn: where.column,
            modifiers: (prop.modifiers || []).map((m) => (typeof m === "string" ? m : m.content)),
          },
          at(prop.loc),
        );
      });
      return Object.assign({ node: "element", tag: node.tag, kind: node.tagType, props, children: node.children.map(describeNode).filter(Boolean) }, at(node.loc));
    }
    case 2:
      return { node: "text", content: node.content };
    case 3:
      return null;
    case 5:
      return Object.assign({ node: "interpolation", content: node.content.content }, at(node.content.loc || node.loc));
    default:
      return Object.assign({ node: "other", type: node.type }, at(node.loc));
  }
}

globalThis.__vue_describe = function (filename, source, options, files) {
  files = files || {};
  try {
    const opened = open(filename, source, options, files);
    if (opened.failed) return JSON.stringify(opened.failed);
    const descriptor = opened.descriptor;
    const answer = { status: "described", deps: opened.deps, diagnostics: opened.diagnostics, bindings: {} };
    if (opened.hasScoped) answer.scope = opened.scopeId;
    if (descriptor.script || descriptor.scriptSetup) {
      const compiled = sfc.compileScript(descriptor, { id: opened.hash, isProd: false, inlineTemplate: false, genDefaultAs: "_sfc_main" });
      for (const name of Object.keys(compiled.bindings || {})) {
        if (!name.startsWith("__")) answer.bindings[name] = compiled.bindings[name];
      }
      const block = descriptor.scriptSetup || descriptor.script;
      answer.script = {
        content: block.content,
        lang: block.lang === "ts" || block.lang === "tsx" ? "ts" : "js",
        line: block.loc.start.line,
        column: block.loc.start.column,
        setup: !!descriptor.scriptSetup,
        plain: !!descriptor.script,
      };
    }
    if (descriptor.template && descriptor.template.ast) {
      answer.template = { children: descriptor.template.ast.children.map(describeNode).filter(Boolean) };
    }
    return JSON.stringify(answer);
  } catch (e) {
    return JSON.stringify({ status: "failed", diagnostics: [diagnostic(e, filename)] });
  }
};

/** The component as a module for Vue's own server renderer: the script plus `ssrRender`. */
globalThis.__vue_ssr_module = function (filename, source, options, files) {
  files = files || {};
  try {
    const opened = open(filename, source, options, files);
    if (opened.failed) return JSON.stringify(opened.failed);
    opened.filename = filename;
    opened.production = !!options.production;
    const assembled = assemble(opened, filename, true);
    if (assembled.failed) return JSON.stringify(assembled.failed);
    return JSON.stringify({ status: "ok", js: assembled.js, lang: opened.lang, deps: opened.deps, diagnostics: opened.diagnostics });
  } catch (e) {
    return JSON.stringify({ status: "failed", diagnostics: [diagnostic(e, filename)] });
  }
};

/**
 * Renders a component module the loader can reach, under the root the
 * client's Vue mounter uses: a wrapper rendering nothing of its own, the
 * props held and the caller's children as the default slot in a region.
 */
globalThis.__vue_render = async function (name, props, children) {
  const { createSSRApp, defineComponent, h } = await import("vue");
  const { renderToString } = await import("vue/server-renderer");
  const component = (await import(name)).default;
  const slots = children == null ? undefined : { default: () => h("sf-s", { "data-sf-children": "", innerHTML: children }) };
  const root = defineComponent({
    name: "SfIsland",
    setup() {
      return () => h(component, props, slots);
    },
  });
  return await renderToString(createSSRApp(root));
};
