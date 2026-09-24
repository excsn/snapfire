import { Link, type Children } from "@snapfire/fsr-authoring/template";

/** The blog's own frame, nested under the portal's header when mounted and the whole page when the site runs alone. */
export default function BlogLayout({ children }: { children: Children }) {
  return (
    <section className="blog">
      <nav className="blog-nav">
        <Link href="/blog">Posts</Link>
        <Link href="/blog/tags">Tags</Link>
      </nav>
      {children}
    </section>
  );
}
