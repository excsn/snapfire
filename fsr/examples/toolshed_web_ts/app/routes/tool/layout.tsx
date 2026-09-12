import { Link, type Children } from "@snapfire/fsr-authoring/template";

export default function ToolLayout({ children }: { children: Children }) {
  return (
    <section className="tool-shell">
      <p className="crumbs">
        <Link href="/">The shelves</Link>
        <span className="sep">/</span>
        <span className="here">This tool</span>
      </p>
      {children}
    </section>
  );
}
