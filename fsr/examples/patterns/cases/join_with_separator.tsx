export default function Page() {
  const parts = ["a", "b", "c"];
  const out: any[] = [];
  parts.forEach((p, i) => {
    if (i > 0) out.push(<span key={"s" + i}> | </span>);
    out.push(<b key={p}>{p}</b>);
  });
  return <p>{out}</p>;
}
