import { tags } from "@src/posts";

export async function load() {
  return { tags: tags().map((group) => ({ tag: group.tag, posts: group.posts.map((post) => ({ slug: post.slug, title: post.title })) })) };
}

export const meta = () => ({ title: "Tags · Blog" });
