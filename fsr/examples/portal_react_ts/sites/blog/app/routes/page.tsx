import type { RootProps } from "@generated/client";
import { Link } from "@snapfire/fsr-authoring/template";

export default function Index({ posts }: RootProps) {
  return (
    <div className="page posts">
      <h1>Posts</h1>
      <ul className="post-list">
        {posts.map((post) => (
          <li key={post.slug}>
            <h2>
              <Link href={`/blog/post/${post.slug}`}>{post.title}</Link>
            </h2>
            <p className="post-meta">
              <time>{post.posted}</time>
              {post.tags.map((tag) => (
                <Link key={tag} href="/blog/tags" className="tag">
                  {tag}
                </Link>
              ))}
            </p>
            <p>{post.summary}</p>
          </li>
        ))}
      </ul>
    </div>
  );
}
