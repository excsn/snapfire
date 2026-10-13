import type { PostSlugProps } from "@generated/client";
import { Link } from "@snapfire/fsr-authoring/template";

export default function PostPage({ post }: PostSlugProps) {
  return (
    <article>
      <h1 className="mb-2 font-serif text-3xl">{post.title}</h1>
      <p className="mb-4 flex items-center gap-2 text-[13px] text-ink-soft">
        <time>{post.posted}</time>
        {post.tags.map((tag) => (
          <Link key={tag} href="/blog/tags" className="tag">
            {tag}
          </Link>
        ))}
      </p>
      <div className="space-y-3 leading-relaxed text-ink [&_code]:rounded [&_code]:bg-accent-soft [&_code]:px-1 [&_code]:text-accent" data-part="body" dangerouslySetInnerHTML={{ __html: post.body }} />
      <p className="mt-6">
        <Link href="/blog" className="text-accent hover:underline">
          All posts
        </Link>
      </p>
    </article>
  );
}
