import type { PostSlugProps } from "@generated/client";
import { Link } from "@snapfire/fsr-authoring/template";

export default async function PostPage({ post }: PostSlugProps) {
  const { default: Body } = await import(`../../../content/posts/${post.file}`);
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
      {post.toc.length > 0 ? (
        <nav className="mb-4 text-sm" data-part="toc">
          {post.toc.map((entry) => (
            <a key={entry.id} href={`#${entry.id}`} className="mr-3 text-accent hover:underline">
              {entry.text}
            </a>
          ))}
        </nav>
      ) : null}
      <div className="space-y-3 leading-relaxed text-ink [&_code]:rounded [&_code]:px-1 [&_table]:w-full [&_td]:border-t [&_td]:border-line [&_th]:text-left" data-part="body">
        <Body components={{ a: Link }} />
      </div>
      <p className="mt-6">
        <Link href="/blog" className="text-accent hover:underline">
          All posts
        </Link>
      </p>
    </article>
  );
}
