import type { Children } from "@snapfire/fsr-authoring/template";
import { Link } from "@snapfire/fsr/components";

export const theme = "editorial";

/** The blog's own frame, nested under the portal's header when mounted and the whole page when the site runs alone. */
export default function BlogLayout({ children }: { children: Children }) {
  return (
    <section className="rounded-xl border border-line bg-card px-6 py-5">
      <nav className="mb-4 flex gap-3 border-b border-line pb-3 font-serif text-lg">
        <Link href="/blog" className="text-accent hover:underline">Posts</Link>
        <Link href="/blog/tags" className="text-accent hover:underline">Tags</Link>
      </nav>
      {children}
    </section>
  );
}
