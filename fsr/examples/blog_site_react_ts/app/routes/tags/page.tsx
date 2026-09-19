import type { TagsProps } from "@generated/client";
import { Link } from "@snapfire/fsr-authoring/template";

export default function Tags({ tags }: TagsProps) {
  return (
    <div className="page tags">
      <h1>Tags</h1>
      {tags.map(({ tag, posts }) => (
        <section key={tag} className="tag-group">
          <h2 className="tag">{tag}</h2>
          <ul>
            {posts.map((post) => (
              <li key={post.slug}>
                <Link href={`/blog/post/${post.slug}`}>{post.title}</Link>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
