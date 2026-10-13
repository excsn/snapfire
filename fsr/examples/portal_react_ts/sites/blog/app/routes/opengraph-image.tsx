export const size = { width: 1200, height: 630 };
export const alt = "The Acme blog";

export default function Image({ count, latest }: { count: number; latest: string }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "space-between", width: "100%", height: "100%", padding: 72, backgroundColor: "#b45309", color: "#fffbeb" }}>
      <span style={{ fontSize: 34, fontWeight: 700 }}>Acme blog</span>
      <h1 style={{ fontSize: 80, fontWeight: 700, margin: 0 }}>{count} posts</h1>
      <p style={{ fontSize: 30, margin: 0 }}>Latest: {latest}</p>
    </div>
  );
}
