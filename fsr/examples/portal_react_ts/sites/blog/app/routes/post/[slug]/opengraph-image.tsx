import type { PostSlugProps } from "@generated/client";

export const size = { width: 1200, height: 630 };

export default function Image({ post }: PostSlugProps) {
  return (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "space-between", width: "100%", height: "100%", padding: 72, backgroundColor: "#fdf6e3", color: "#1f2937" }}>
      <span style={{ fontSize: 30, fontWeight: 700, color: "#b45309" }}>Acme blog</span>
      <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
        <h1 style={{ fontSize: 72, fontWeight: 700, lineHeight: 1.1, margin: 0, lineClamp: 2 }}>{post.title}</h1>
        <p style={{ fontSize: 32, lineHeight: 1.4, margin: 0, color: "#4b5563", lineClamp: 2 }}>{post.summary}</p>
      </div>
      <div style={{ display: "flex", gap: 16, fontSize: 26 }}>
        <span>{post.posted}</span>
        {post.tags.map((tag) => (
          <span key={tag} style={{ padding: "4px 16px", borderRadius: 999, backgroundColor: "#fde68a" }}>{tag}</span>
        ))}
      </div>
    </div>
  );
}
