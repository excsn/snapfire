import type { RootProps } from "@generated/client";
import { Reserved } from "@src/reserved";
import { ToolRow } from "@src/ui/ToolRow";

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
      <Reserved value={reserved}>
        <ol className="tool-list">
          {tools.map((tool) => (
            <ToolRow key={tool.id} tool={tool} />
          ))}
        </ol>
      </Reserved>
      {tools.length === 0 ? <p className="quiet">Nothing on that shelf.</p> : null}
      <p className="note">The shelf chips are htmx: each one fetches this page as a fragment and swaps it in place. The tool names are links the navigator handles.</p>
    </section>
  );
}
