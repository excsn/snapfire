import type { PostSlugProps } from "@generated/client";
import { Link } from "@snapfire/fsr-authoring/template";

export default function PostPage({ post }: PostSlugProps) {
  return (
    <article>
      <h1 className="mb-2 font-serif text-3xl">{post.title}</h1>
      <p className="mb-4 flex items-center gap-2 text-[13px] text-slate-500">
        <time>{post.posted}</time>
        {post.tags.map((tag) => (
          <Link key={tag} href="/blog/tags" className="tag">
            {tag}
          </Link>
        ))}
      </p>
      <div className="space-y-3 leading-relaxed text-slate-800 [&_code]:rounded [&_code]:bg-violet-50 [&_code]:px-1 [&_code]:text-violet-900" data-part="body" dangerouslySetInnerHTML={{ __html: post.body }} />
      <p className="mt-6">
        <Link href="/blog" className="text-violet-700 hover:underline">
          All posts
        </Link>
      </p>
    </article>
  );
}
