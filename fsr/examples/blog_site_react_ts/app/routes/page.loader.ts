import { posts } from "@src/posts";

export async function load() {
  return { posts: posts.map((post) => ({ slug: post.slug, title: post.title, posted: post.posted, tags: post.tags, summary: post.summary })) };
}

export const meta = () => ({ title: "Posts · Blog" });
