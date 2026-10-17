export default function Page() {
  const specs = { weight: "1kg", color: "red" };
  return <dl>{Object.entries(specs).map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{v}</dd></div>)}</dl>;
}
