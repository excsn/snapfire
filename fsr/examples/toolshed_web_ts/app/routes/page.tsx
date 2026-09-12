import { Link } from "@snapfire/fsr-authoring/template";

import type { RootProps } from "@generated/client";

export default function ToolsPage({ category, categories, tools, reserved }: RootProps) {
  return (
    <section className="page tools">
      <h2>On the shelves</h2>
      <nav className="categories">
        <a href="/" className={category === "all" ? "chip chip-on" : "chip"} hx-get="/?__fragment" hx-target="closest .page" hx-swap="outerHTML" data-sf-native>
          Everything
        </a>
        {categories.map((name) => (
          <a
            key={name}
            href={`/?category=${encodeURIComponent(name)}`}
            className={category === name ? "chip chip-on" : "chip"}
            hx-get={`/?category=${encodeURIComponent(name)}&__fragment`}
            hx-target="closest .page"
            hx-swap="outerHTML"
            data-sf-native
          >
            {name}
          </a>
        ))}
      </nav>
      <ol className="tool-list">
        {tools.map((tool) => (
          <li key={tool.id} className="tool-row">
            <span className="what">
              <Link href={`/tool/${tool.id}`} className="tool-title">
                {tool.name}
              </Link>
              <span className="by">
                {tool.keeper}'s, £{tool.deposit} deposit, {tool.days} days
              </span>
            </span>
            <span className={`category category-${tool.category.toLowerCase()}`}>{tool.category}</span>
            {reserved.includes(tool.id) ? <span className="kept">reserved</span> : <span className="kept kept-off" />}
          </li>
        ))}
      </ol>
      {tools.length === 0 ? <p className="quiet">Nothing on that shelf.</p> : null}
      <p className="note">The shelf chips are htmx: each one fetches this page as a fragment and swaps it in place. The tool names are links the navigator handles.</p>
    </section>
  );
}
