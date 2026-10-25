import type { Children } from "@snapfire/fsr/template";
import { Link } from "@snapfire/fsr/components";

/** The status site's frame, nested under the portal's header when mounted and the whole page when the site runs alone. */
export default function StatusLayout({ children }: { children: Children }) {
  return (
    <section className="rounded-xl border border-line bg-card px-6 py-5">
      <nav className="mb-4 border-b border-line pb-3">
        <Link href="/status" className="font-semibold text-accent">
          Status
        </Link>
      </nav>
      {children}
    </section>
  );
}
