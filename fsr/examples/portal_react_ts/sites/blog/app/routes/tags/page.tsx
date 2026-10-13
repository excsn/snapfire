import type { TagsProps } from "@generated/client";
import { Link } from "@snapfire/fsr-authoring/template";

export default function Tags({ tags }: TagsProps) {
  return (
    <div>
      <h1 className="mb-4 font-serif text-3xl">Tags</h1>
      {tags.map(({ tag, posts }) => (
        <section key={tag} className="mb-5" data-part="tag">
          <h2 className="tag mb-1.5">{tag}</h2>
          <ul className="list-disc pl-5">
            {posts.map((post) => (
              <li key={post.slug}>
                <Link href={`/blog/post/${post.slug}`} className="text-accent hover:underline">
                  {post.title}
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
