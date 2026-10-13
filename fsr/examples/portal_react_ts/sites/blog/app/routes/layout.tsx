import { Link, type Children } from "@snapfire/fsr-authoring/template";

/** The blog's own frame, nested under the portal's header when mounted and the whole page when the site runs alone. */
export default function BlogLayout({ children }: { children: Children }) {
  return (
    <section className="rounded-xl border border-violet-200 bg-white px-6 py-5 shadow-sm">
      <nav className="mb-4 flex gap-3 border-b border-violet-100 pb-3 font-serif text-lg">
        <Link href="/blog" className="text-violet-700 hover:underline">Posts</Link>
        <Link href="/blog/tags" className="text-violet-700 hover:underline">Tags</Link>
      </nav>
      {children}
    </section>
  );
}
