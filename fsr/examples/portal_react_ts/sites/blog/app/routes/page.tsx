import type { RootProps } from "@generated/client";
import { Link } from "@snapfire/fsr/components";

export default function Index({ posts }: RootProps) {
  return (
    <div>
      <h1 className="mb-4 font-serif text-3xl">Posts</h1>
      <ul className="grid gap-5" data-part="posts">
        {posts.map((post) => (
          <li key={post.slug}>
            <h2 className="mb-1 font-serif text-xl">
              <Link href={`/blog/post/${post.slug}`} className="text-accent hover:underline">
                {post.title}
              </Link>
            </h2>
            <p className="mb-1.5 flex items-center gap-2 text-[13px] text-ink-soft">
              <time>{post.posted}</time>
              {post.tags.map((tag) => (
                <Link key={tag} href="/blog/tags" className="tag">
                  {tag}
                </Link>
              ))}
            </p>
            <p className="text-ink">{post.summary}</p>
          </li>
        ))}
      </ul>
    </div>
  );
}
