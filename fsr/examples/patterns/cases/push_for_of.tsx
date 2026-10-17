export default function Page() {
  const rows = [{ id: "a", on: true }, { id: "b", on: false }, { id: "c", on: true }];
  const out = [];
  for (const row of rows) {
    if (!row.on) continue;
    out.push(<li key={row.id}>{row.id}</li>);
  }
  return <ul>{out}</ul>;
}
