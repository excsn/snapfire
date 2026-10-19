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

  globalThis.__svelte_compile = function (filename, source, options) {
    try {
      const refused = unsupported(source, filename);
      if (refused.length) return JSON.stringify({ status: "failed", diagnostics: refused });
      const result = svelte.compile(source, { filename, generate: "client", dev: !options.production, css: "external" });
      return JSON.stringify({
        status: "ok",
        js: result.js.code,
        lang: "js",
        css: result.css ? result.css.code : null,
        deps: [],
        diagnostics: result.warnings.map((w) => diagnostic(w, filename, "warning")),
      });
    } catch (e) {
      return JSON.stringify({ status: "failed", diagnostics: [diagnostic(e, filename)] });
    }
  };
})();
