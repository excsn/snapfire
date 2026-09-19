import { fail } from "@snapfire/fsr";
import type { Ctx } from "@snapfire/fsr";
import { posts } from "@src/posts";

export async function load({ params }: Ctx<"/post/{slug}">) {
  const matches = posts.filter((candidate) => candidate.slug === params.slug);
  if (matches.length === 0) fail("not_found", "there is no post with that slug");
  const post = matches[0];
  return { post };
}

export const meta = ({ data }: { data: { post: { title: string; summary: string } } }) => ({ title: `${data.post.title} · Blog`, description: data.post.summary });

export const paths = () => posts.map((post) => ({ slug: post.slug }));
