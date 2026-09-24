export type Post = {
  slug: string;
  title: string;
  posted: string;
  tags: string[];
  summary: string;
  /** Markup the application produced at build time. The renderer writes it as it stands. */
  body: string;
};

export const posts: Post[] = [
  {
    slug: "why-a-plan-file",
    title: "Why the server reads a plan file",
    posted: "2026-09-02",
    tags: ["design", "runtime"],
    summary: "A route is data the host can read, which is what lets it prerender, cache and reason about a page.",
    body: "<p>A loader is lowered to a form the host runs itself, so nothing on the serving path evaluates JavaScript. The plan file is the whole description of a route: its pattern, its loader, its layout and what each one reads.</p><p>That is why a page whose loader reads nothing from the request can be written to disk once. The host can tell from the plan, without running anything.</p>",
  },
  {
    slug: "a-site-is-a-mount",
    title: "A site is a mount, not a dependency",
    posted: "2026-09-09",
    tags: ["sites", "design"],
    summary: "The portal mounts this blog from its build output. Nothing here imports the portal and nothing there imports this.",
    body: "<p>The shell reads the site's artifact from a path and nests its routes under the shell's root layout. One session, one store, one navigation, and the site still runs alone on its own port.</p><p>This blog vendors nothing: React comes from the shell contract the site was built against.</p>",
  },
  {
    slug: "static-under-a-shell",
    title: "Static pages under a shell that reads the session",
    posted: "2026-09-16",
    tags: ["runtime", "prerender"],
    summary: "The portal's layout reads the session for its header, so no document under /blog can be written whole. The pages are written anyway.",
    body: "<p>The shell's <code>fsr prerender</code> renders each blog page once and keeps the rendered subtree beside the loads. A request evaluates the layout for the visitor and splices the page in.</p><p>Running alone, the same routes are documents on disk: the index, the tag list and one file per post, from the slugs the post loader names.</p>",
  },
];

/** Every tag a post carries, in the order the tag list shows them. */
export const tagNames: string[] = ["design", "prerender", "runtime", "sites"];

export const tags = (): { tag: string; posts: Post[] }[] => tagNames.map((tag) => ({ tag, posts: posts.filter((post) => post.tags.includes(tag)) }));
