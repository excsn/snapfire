"""Port oxlint's rules into snapfire_lint's JS front end and regenerate the rule registry.

usage:
  python3 -I lint/scripts/port_oxlint.py port OXC_CHECKOUT PLUGIN RULE...
  python3 -I lint/scripts/port_oxlint.py port OXC_CHECKOUT PLUGIN --recommended
  python3 -I lint/scripts/port_oxlint.py copy OXC_CHECKOUT SOURCE DEST
  python3 -I lint/scripts/port_oxlint.py registry

OXC_CHECKOUT is a checkout of github.com/oxc-project/oxc at the `crates_vX.Y.Z` tag whose oxc crates
lint/Cargo.toml depends on. RULE is a path under crates/oxc_linter/src/rules/PLUGIN/: `no_debugger.rs`, or
`no_unused_vars/mod.rs` for a rule with its own directory. `--recommended` ports every rule of PLUGIN that
oxlint's recommended set turns on. `port` writes into lint/src/js/rules/PLUGIN/ and ends by regenerating the
registry. A rule oxlint runs through tsgolint has no Rust body and is skipped. `copy` brings one framework
file, SOURCE under crates/oxc_linter/src/, to DEST under lint/src/js/ with the same rewrites. `registry`
rewrites lint/src/js/rules/mod.rs from the rule files present, with `rules/shared`, the code oxlint's rules share, when it is there. A plugin's directory is oxlint's name for it,
`jsx_a11y`; its rule ids use ESLint's, `jsx-a11y/alt-text`.

A ported file starts with the MIT notice and the path it came from. The script rewrites `crate::` to
`crate::js::`, `declare_oxc_lint!` to `declare_rule!`, removes `schemars` and switches tests from
`test_and_snapshot` to `test`. What it cannot rewrite fails to compile and is fixed by hand.
"""

import os
import re
import subprocess
import sys

RULES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src", "js", "rules")

NOTICE = "// Ported from oxlint ({path}) at oxc {tag}, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.\n\n"


def kebab(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower()


def fix_expr(fix):
    simple = {
        "none": "RuleFixMeta::None",
        "pending": "RuleFixMeta::FixPending",
        "fix": "RuleFixMeta::Fixable(FixKind::SafeFix)",
        "suggestion": "RuleFixMeta::Fixable(FixKind::Suggestion)",
    }
    if fix in simple:
        return simple[fix]
    conditional = False
    kinds = []
    for seg in fix.split("_"):
        if seg == "conditional":
            conditional = True
        elif seg in ("safe", "and", "or"):
            continue
        else:
            kind = {"fix": "FixKind::Fix", "fixes": "FixKind::Fix", "suggestion": "FixKind::Suggestion", "suggestions": "FixKind::Suggestion", "dangerous": "FixKind::Dangerous"}[seg]
            if kind not in kinds:
                kinds.append(kind)
    expr = kinds[0]
    for kind in kinds[1:]:
        expr = f"{expr}.union({kind})"
    return f"RuleFixMeta::{'Conditional' if conditional else 'Fixable'}({expr})"


FIX_WORDS = re.compile(r"^(none|pending|fix|suggestion|(fix|suggestion|dangerous|conditional|safe|and|or)(_(fix|fixes|suggestion|suggestions|dangerous|conditional|safe|and|or))+)$")


def port_declare(text, plugin):
    m = re.search(r"declare_oxc_lint!\(\n(.*?)\n\);", text, re.S)
    if not m:
        raise SystemExit("no declare_oxc_lint! block")
    body = m.group(1)
    docs = []
    rest = []
    for line in body.splitlines():
        stripped = line.strip()
        if stripped.startswith("///"):
            docs.append(line)
        else:
            rest.append(re.sub(r"//.*$", "", line))
    rest = " ".join(rest)
    parts = []
    depth = 0
    cur = ""
    in_str = False
    for ch in rest:
        if ch == '"' and not cur.endswith("\\"):
            in_str = not in_str
        if ch == "," and not in_str and depth == 0:
            parts.append(cur.strip())
            cur = ""
            continue
        cur += ch
    if cur.strip():
        parts.append(cur.strip())
    name, declared_plugin, category = parts[0], parts[1], parts[2]
    recommended = False
    fix = "none"
    shared_docs = None
    for part in parts[3:]:
        if part == "recommended":
            recommended = True
        elif part.startswith("docs ="):
            shared_docs = part.split("=", 1)[1].strip()
        elif part.startswith("fix ="):
            fix = part.split("=", 1)[1].strip()
        elif "=" in part:
            continue
        elif FIX_WORDS.match(part):
            fix = part
        else:
            raise SystemExit(f"unknown declare_oxc_lint! argument `{part}`")
    if declared_plugin.replace("_", "-") != plugin.replace("_", "-"):
        raise SystemExit(f"{name} is in plugin {declared_plugin}")
    tail = f",\n    docs = {shared_docs}" if shared_docs else ""
    new = "declare_rule!(\n" + "\n".join(docs) + f"\n    {name},\n    \"{plugin.replace('_', '-')}\",\n    \"{kebab(name)}\",\n    {category.capitalize()},\n    recommended = {'true' if recommended else 'false'},\n    fix = {fix_expr(fix)}{tail},\n);"
    return text[: m.start()] + new + text[m.end() :], name


def strip_schemars(text):
    text = text.replace("use schemars::JsonSchema;\n", "")
    text = re.sub(r"\n[ \t]*#\[(schemars|validate)\((?:[^()]|\([^()]*\))*\)\]", "", text)
    text = re.sub(r",\s*JsonSchema\b", "", text)
    text = re.sub(r"\bJsonSchema,\s*", "", text)
    return text


def strip_orphan_serde(text):
    out = []
    pos = 0
    for m in re.finditer(r"#\[derive\(([^)]*)\)\]", text):
        if m.start() < pos or "Deserialize" in m.group(1) or "Serialize" in m.group(1):
            continue
        open_at = text.find("{", m.end())
        semi = text.find(";", m.end())
        if open_at == -1 or (semi != -1 and semi < open_at):
            continue
        depth = 0
        k = open_at
        while True:
            if text[k] == "{":
                depth += 1
            elif text[k] == "}":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        item = re.sub(r"\n[ \t]*#\[serde\((?:[^()]|\([^()]*\))*\)\]", "", text[m.start() : k + 1])
        out.append(text[pos : m.start()])
        out.append(item)
        pos = k + 1
    out.append(text[pos:])
    return "".join(out)


def port_file(src, dest, rel, plugin, declare, tag, origin=None):
    text = open(src).read()
    text = re.sub(r"\bcrate::(?!js::)", "crate::js::", text)
    text = re.sub(r"\bconfig::GlobalValue\b", "globals::GlobalValue", text)
    text = re.sub(r"\bconfig::(settings::\w+::)?ReactVersion\b", "settings::ReactVersion", text)
    text = text.replace("crate::js::rules::ContextHost", "crate::js::context::ContextHost")
    name = None
    if declare:
        text = text.replace("use oxc_macros::declare_oxc_lint;\n", "use crate::js::declare_rule;\n")
        text, name = port_declare(text, plugin)
    text = strip_schemars(text)
    text = strip_orphan_serde(text)
    text = text.replace(".test_and_snapshot()", ".test()")
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    open(dest, "w").write(NOTICE.format(path=origin or f"crates/oxc_linter/src/rules/{rel}", tag=tag) + text)
    return name


def oxc_tag(checkout):
    tag = subprocess.run(["git", "-C", checkout, "describe", "--tags", "--exact-match"], capture_output=True, text=True).stdout.strip()
    if not tag.startswith("crates_v"):
        sys.exit(f"{checkout} is not at a crates_v tag; check out the tag lint/Cargo.toml's oxc version names")
    wanted = re.search(r'oxc_ast = "([0-9.]+)"', open(os.path.join(RULES, "..", "..", "..", "Cargo.toml")).read())
    if wanted and not tag.removeprefix("crates_v").startswith(wanted.group(1)):
        sys.exit(f"{checkout} is at {tag}; lint/Cargo.toml depends on oxc {wanted.group(1)}")
    return tag


def recommended(plugin_dir):
    out = []
    for entry in sorted(os.listdir(plugin_dir)):
        path = os.path.join(plugin_dir, entry)
        rel = f"{entry}/mod.rs" if os.path.isdir(path) else entry
        if not rel.endswith(".rs") or not os.path.exists(os.path.join(plugin_dir, rel)):
            continue
        m = re.search(r"declare_oxc_lint!\((.*?)\n\);", open(os.path.join(plugin_dir, rel)).read(), re.S)
        if m and re.search(r"^\s*recommended,", m.group(1), re.M) and "(tsgolint)" not in m.group(1):
            out.append(rel)
    return out


def port(checkout, plugin, rules):
    tag = oxc_tag(checkout)
    src_dir = os.path.join(checkout, "crates", "oxc_linter", "src", "rules")
    if rules == ["--recommended"]:
        rules = recommended(os.path.join(src_dir, plugin))
    for rule in rules:
        rel = f"{plugin}/{rule}"
        if rel.endswith("/mod.rs"):
            folder = os.path.dirname(rel)
            for base, _, files in sorted(os.walk(os.path.join(src_dir, folder))):
                for file in sorted(files):
                    if file.endswith(".rs"):
                        inner = os.path.relpath(os.path.join(base, file), src_dir)
                        port_file(os.path.join(src_dir, inner), os.path.join(RULES, inner), inner, plugin, inner == f"{folder}/mod.rs", tag)
        else:
            port_file(os.path.join(src_dir, rel), os.path.join(RULES, rel), rel, plugin, True, tag)
        print(f"ported {rel}")
    registry()


DECLARED = re.compile(r'declare_rule!\(\n(?:\s*///.*\n)*\s*(\w+),\s*\n\s*"([\w-]+)",\s*\n\s*"([\w-]+)"')


def copy(checkout, source, dest):
    tag = oxc_tag(checkout)
    js = os.path.join(RULES, "..")
    port_file(os.path.join(checkout, "crates", "oxc_linter", "src", source), os.path.join(js, dest), source, "", False, tag, f"crates/oxc_linter/src/{source}")
    print(f"copied {source} to {dest}")


def registry():
    plugins = {}
    for plugin in sorted(os.listdir(RULES)):
        plugin_dir = os.path.join(RULES, plugin)
        if not os.path.isdir(plugin_dir):
            continue
        for entry in sorted(os.listdir(plugin_dir)):
            path = os.path.join(plugin_dir, entry, "mod.rs") if os.path.isdir(os.path.join(plugin_dir, entry)) else os.path.join(plugin_dir, entry)
            if not path.endswith(".rs") or not os.path.exists(path):
                continue
            m = DECLARED.search(open(path).read())
            if m:
                module = entry.removesuffix(".rs")
                plugins.setdefault(plugin, []).append((f"{m.group(2)}/{m.group(3)}", module, m.group(1)))
    mods = "".join(f"pub mod {plugin} {{\n" + "".join(f"  pub mod {module};\n" for _, module, _ in sorted(rules, key=lambda r: r[1])) + "}\n\n" for plugin, rules in plugins.items())
    if os.path.exists(os.path.join(RULES, "shared", "mod.rs")):
        mods = "pub mod shared;\n\n" + mods
    rows = sorted((id, f"  entry::<{plugin}::{module}::{name}>(),\n") for plugin, rules in plugins.items() for id, module, name in rules)
    text = f"""//! The rules, by plugin. Each file is ported from oxlint and keeps its notice. Written by
//! lint/scripts/port_oxlint.py registry.

use super::{{Entry, entry}};

{mods}/// Every rule, sorted by id.
pub(crate) static RULES: &[Entry] = &[
{"".join(row for _, row in rows)}];
"""
    open(os.path.join(RULES, "mod.rs"), "w").write(text)
    print(f"registry: {sum(len(r) for r in plugins.values())} rules")


def main():
    if sys.argv[1:2] == ["registry"]:
        registry()
    elif sys.argv[1:2] == ["copy"] and len(sys.argv) == 5:
        copy(sys.argv[2], sys.argv[3], sys.argv[4])
    elif sys.argv[1:2] == ["port"] and len(sys.argv) > 4:
        port(sys.argv[2], sys.argv[3], sys.argv[4:])
    else:
        sys.exit(__doc__)


main()
