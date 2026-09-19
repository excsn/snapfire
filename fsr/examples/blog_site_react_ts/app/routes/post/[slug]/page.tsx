import type { PostSlugProps } from "@generated/client";
import { Link } from "@snapfire/fsr-authoring/template";

export default function PostPage({ post }: PostSlugProps) {
  return (
    <article className="page post">
      <h1>{post.title}</h1>
      <p className="post-meta">
        <time>{post.posted}</time>
        {post.tags.map((tag) => (
          <Link key={tag} href="/blog/tags" className="tag">
            {tag}
          </Link>
        ))}
      </p>
      <div className="post-body" dangerouslySetInnerHTML={{ __html: post.body }} />
      <p>
        <Link href="/blog">All posts</Link>
      </p>
    </article>
  );
}
