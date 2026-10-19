(function () {
  const svelte = globalThis.svelte;

  if (typeof globalThis.console === "undefined") {
    const quiet = function () {};
    globalThis.console = { log: quiet, warn: quiet, error: quiet, info: quiet, debug: quiet };
  }

  globalThis.__svelte_version = function () {
    return svelte.VERSION;
  };

  /** Svelte's `start.line` is 1-based and `start.column` 0-based; the wire's column is 1-based. */
  function diagnostic(error, filename, severity) {
    const start = error && error.start;
    return {
      severity: severity || "error",
      message: String((error && error.message) || error),
      file: filename,
      line: start ? start.line : undefined,
      column: start ? start.column + 1 : undefined,
    };
  }

  const CARRIED = { script: ["ts", "js"], style: ["css"] };

  /**
   * A block in a language the plugin cannot hand on, named rather than
   * emitted wrong. Read from the opening tags rather than the parse, since
   * Svelte's parser reads the style block as CSS and fails on SCSS first.
   */
  function unsupported(source, filename) {
    const refused = [];
    const tags = /<(script|style)\b([^>]*)>/g;
    let tag;
    while ((tag = tags.exec(source))) {
      const lang = /\blang\s*=\s*["']?([\w-]+)/.exec(tag[2]);
      if (lang && !CARRIED[tag[1]].includes(lang[1])) {
        const line = source.slice(0, tag.index).split("\n").length;
        refused.push({ severity: "error", message: tag[1] + ' lang="' + lang[1] + '" needs a compiler this plugin does not carry', file: filename, line });
      }
    }
    return refused;
  }

  /** A runes module, `name.svelte.ts` or `name.svelte.js`, rather than a component. */
  function isModule(filename) {
    return /\.svelte\.(ts|js)$/i.test(filename);
  }

  const UNDESCRIBED = "snapfirec-svelte does not describe a component yet; it renders in the browser";

  /**
   * The component's scripts at their own byte offsets, everything else
   * blanked to spaces, so the host can follow what they import. The error
   * diagnostic keeps the component foreign.
   */
  globalThis.__svelte_describe = function (filename, source) {
    const refused = unsupported(source, filename);
    if (refused.length) return JSON.stringify({ status: "failed", diagnostics: refused });
    let root;
    try {
      root = svelte.parse(source, { filename, modern: true });
    } catch (e) {
      return JSON.stringify({ status: "failed", diagnostics: [diagnostic(e, filename)] });
    }
    const blocks = [root.module, root.instance].filter(Boolean);
    const undescribed = { severity: "error", message: UNDESCRIBED, file: filename };
    if (!blocks.length) return JSON.stringify({ status: "described", diagnostics: [undescribed] });
    const kept = blocks.map((block) => [block.content.start, block.content.end]);
    // Svelte's offsets count UTF-16 units; the blanks count UTF-8 bytes, so
    // every byte offset the host reads matches the file.
    let content = "";
    let at = 0;
    for (const ch of source) {
      if (ch === "\n" || kept.some(([start, end]) => at >= start && at < end)) {
        content += ch;
      } else {
        const code = ch.codePointAt(0);
        content += " ".repeat(code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4);
      }
      at += ch.length;
    }
    const typed = blocks.some((block) => block.attributes.some((a) => a.name === "lang" && Array.isArray(a.value) && a.value.some((v) => v.data === "ts")));
    return JSON.stringify({ status: "described", script: { content, lang: typed ? "ts" : "js", line: 1, column: 1 }, diagnostics: [undescribed] });
  };

  globalThis.__svelte_compile = function (filename, source, options) {
    try {
      let result;
      if (isModule(filename)) {
        result = svelte.compileModule(source, { filename, generate: "client", dev: !options.production });
      } else {
        const refused = unsupported(source, filename);
        if (refused.length) return JSON.stringify({ status: "failed", diagnostics: refused });
        result = svelte.compile(source, { filename, generate: "client", dev: !options.production, css: "external" });
      }
      return JSON.stringify({
        status: "ok",
        js: result.js.code,
        lang: "js",
        css: result.css ? result.css.code : null,
        source_map: options.source_map && result.js.map ? result.js.map.toString() : null,
        deps: [],
        diagnostics: result.warnings.map((w) => diagnostic(w, filename, "warning")),
      });
    } catch (e) {
      return JSON.stringify({ status: "failed", diagnostics: [diagnostic(e, filename)] });
    }
  };
})();
