import { Link, type Children } from "@snapfire/fsr-authoring/template";

/** The status site's frame, nested under the portal's header when mounted and the whole page when the site runs alone. */
export default function StatusLayout({ children }: { children: Children }) {
  return (
    <section className="status">
      <nav className="status-nav">
        <Link href="/status">Status</Link>
      </nav>
      {children}
    </section>
  );
}
