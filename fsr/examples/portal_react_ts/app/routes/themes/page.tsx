import type { ThemesProps } from "@generated/client";

import { ThemePick } from "@src/ui/ThemePick";
import { Vendored } from "@src/ui/Vendored";

const cells = [
  { theme: "default", scheme: "light", label: "Default", note: "The portal's own blue." },
  { theme: "default", scheme: "dark", label: "Default", note: "The portal's own blue." },
  { theme: "ocean", scheme: "light", label: "Ocean", note: "Teal on a pale canvas." },
  { theme: "ocean", scheme: "dark", label: "Ocean", note: "Teal on a pale canvas." },
  { theme: "forest", scheme: "light", label: "Forest", note: "Green, set in a serif." },
  { theme: "forest", scheme: "dark", label: "Forest", note: "Green, set in a serif." },
];

export default function ThemesPage({ theme, scheme }: ThemesProps) {
  return (
    <div className="grid gap-6">
      <section>
        <h1>Themes</h1>
        <p className="text-ink-soft">Each cell is drawn in its own theme and scheme. Your pick holds on every site the portal mounts, except billing, which keeps its ledger theme.</p>
      </section>
      <div className="flex flex-wrap gap-2">
        <ThemePick theme="" label="Use each page's own theme" />
        <ThemePick scheme="" label="Match the system's light or dark" />
      </div>
      <section className="grid gap-2">
        <h2 className="text-lg font-semibold">Vendored components</h2>
        <p className="text-ink-soft">These come from packages under <code>vendor/@acme/</code>. Their classes reach the stylesheet from the package's class list or from its module.</p>
        <Vendored />
      </section>
      <div data-sf-theme="default" className="grid gap-4 sm:grid-cols-2">
        {cells.map((cell) => (
          <figure
            key={`${cell.theme}-${cell.scheme}`}
            data-sf-theme={cell.theme}
            data-sf-scheme={cell.scheme}
            data-preview={`${cell.theme}-${cell.scheme}`}
            aria-current={cell.theme === theme && cell.scheme === scheme ? "true" : undefined}
            className="m-0 grid gap-3 rounded-[10px] border border-line bg-canvas p-4 font-sans text-ink aria-[current]:outline-2 aria-[current]:outline-accent"
          >
            <div className="rounded-md border border-line bg-card p-3">
              <p className="m-0 font-semibold">
                {cell.label}, {cell.scheme}
              </p>
              <p className="m-0 text-[13px] text-ink-soft">{cell.note}</p>
              <p className="m-0 mt-2 text-[13px]">
                <a href="/themes">A link</a> beside <code>code</code>
              </p>
            </div>
            <figcaption className="flex items-center gap-2">
              <span className="pill">A pill</span>
              <ThemePick theme={cell.theme} scheme={cell.scheme} label="Use this" />
            </figcaption>
          </figure>
        ))}
      </div>
    </div>
  );
}
