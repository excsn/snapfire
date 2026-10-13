import type { PostSlugProps } from "@generated/client";

export const size = { width: 1200, height: 630 };

export default function Image({ post }: PostSlugProps) {
  return (
    <div className="flex h-full w-full flex-col justify-between bg-amber-50 p-[72px] text-gray-800">
      <span className="text-[30px] font-bold text-amber-700">Acme blog</span>
      <div className="flex flex-col gap-6">
        <h1 className="m-0 text-[72px]/[1.1] font-bold" style={{ lineClamp: 2 }}>
          {post.title}
        </h1>
        <p className="m-0 text-[32px]/[1.4] text-gray-600" style={{ lineClamp: 2 }}>
          {post.summary}
        </p>
      </div>
      <div className="flex gap-4 text-[26px]">
        <span>{post.posted}</span>
        {post.tags.map((tag) => (
          <span key={tag} className="rounded-full bg-amber-200 px-4 py-1">
            {tag}
          </span>
        ))}
      </div>
    </div>
  );
}
