import type { Children } from "@snapfire/fsr-authoring/template";

/** A note set apart from the post's text. */
export function Aside({ label, children }: { label: string; children?: Children }) {
  return (
    <aside className="my-4 rounded-lg border border-line bg-accent-soft px-4 py-3 text-sm" data-part="aside">
      <strong className="mr-2 text-accent">{label}</strong>
      {children}
    </aside>
  );
}
