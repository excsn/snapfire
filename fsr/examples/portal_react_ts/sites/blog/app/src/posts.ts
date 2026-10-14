import { posts as files } from "@generated/content";

export type Post = {
  slug: string;
  /** The file under `content/posts/` the post page imports. */
  file: string;
  title: string;
  posted: string;
  tags: string[];
  summary: string;
  /** The post's headings, as the build listed them. */
  toc: { depth: number; id: string; text: string }[];
  /** The post's first paragraph as plain text. */
  excerpt: string | null;
};

/** Every post under `content/posts/`, oldest first. */
export const posts: Post[] = files
  .map((entry) => ({ slug: entry.slug, file: entry.file, title: entry.meta.title, posted: entry.meta.posted, tags: entry.meta.tags, summary: entry.meta.summary, toc: entry.toc, excerpt: entry.excerpt }))
  .sort((a, b) => (a.posted < b.posted ? -1 : 1));

/** Every tag a post carries, in the order the tag list shows them. */
export const tagNames: string[] = ["design", "prerender", "runtime", "sites"];

export const tags = (): { tag: string; posts: Post[] }[] => tagNames.map((tag) => ({ tag, posts: posts.filter((post) => post.tags.includes(tag)) }));
