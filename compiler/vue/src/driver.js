import * as sfc from "@vue/compiler-sfc";

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

globalThis.__vue_compile = function (filename, source, options) {
  const diagnostics = [];
  const deps = [];
  let bindings;
  let lang = "js";
  try {
    const parsed = sfc.parse(source, { filename, sourceMap: !!options.source_map });
    if (parsed.errors.length) {
      return JSON.stringify({ status: "failed", diagnostics: parsed.errors.map((e) => diagnostic(e, filename)) });
    }
    const descriptor = parsed.descriptor;
    const hash = scopeHash(filename, source);
    const scopeId = "data-v-" + hash;
    const hasScoped = descriptor.styles.some((s) => s.scoped);

    for (const block of [descriptor.script, descriptor.scriptSetup].filter(Boolean)) {
      const refused = unsupported(block, "script", filename);
      if (refused) diagnostics.push(refused);
      if (block.src) deps.push(block.src);
      if (block.lang === "ts" || block.lang === "tsx") lang = "ts";
    }
    for (const style of descriptor.styles) {
      const refused = unsupported(style, "style", filename);
      if (refused) diagnostics.push(refused);
      if (style.src) deps.push(style.src);
    }
    if (descriptor.template && descriptor.template.src) deps.push(descriptor.template.src);
    if (diagnostics.some((d) => d.severity === "error")) {
      return JSON.stringify({ status: "failed", diagnostics });
    }

    const parts = [];
    if (descriptor.script || descriptor.scriptSetup) {
      const script = sfc.compileScript(descriptor, {
        id: hash,
        isProd: !!options.production,
        inlineTemplate: false,
        genDefaultAs: "_sfc_main",
      });
      parts.push(script.content);
      bindings = script.bindings;
    } else {
      parts.push("const _sfc_main = {};");
    }

    if (descriptor.template) {
      const template = sfc.compileTemplate({
        source: descriptor.template.content,
        filename,
        id: hash,
        scoped: hasScoped,
        slotted: descriptor.slotted,
        compilerOptions: { bindingMetadata: bindings, scopeId: hasScoped ? scopeId : null },
      });
      if (template.errors.length) {
        return JSON.stringify({ status: "failed", diagnostics: template.errors.map((e) => diagnostic(e, filename)) });
      }
      for (const tip of template.tips || []) diagnostics.push(diagnostic({ message: tip }, filename, "warning"));
      parts.push(template.code);
      parts.push("_sfc_main.render = render;");
    }
    if (hasScoped) parts.push("_sfc_main.__scopeId = " + JSON.stringify(scopeId) + ";");
    parts.push("_sfc_main.__file = " + JSON.stringify(filename) + ";");
    parts.push("export default _sfc_main;");

    const styles = [];
    for (const style of descriptor.styles) {
      const compiled = sfc.compileStyle({
        source: style.content,
        filename,
        id: scopeId,
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
      js: parts.join("\n"),
      lang,
      css: styles.length ? styles.join("\n") : undefined,
      deps,
      diagnostics,
    });
  } catch (e) {
    return JSON.stringify({
      status: "failed",
      diagnostics: [diagnostic(e, filename)].concat(diagnostics),
    });
  }
};
