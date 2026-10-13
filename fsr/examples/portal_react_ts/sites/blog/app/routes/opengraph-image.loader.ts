import { posts } from "@src/posts";

export async function load() {
  return { count: posts.length, latest: posts[0].title };
}
