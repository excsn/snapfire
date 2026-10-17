type Row = { name: string; qty: number };
const columns = [
  { key: "name", render: (row: Row) => <b>{row.name}</b> },
  { key: "qty", render: (row: Row) => <span>{row.qty}</span> },
];
export default function Page() {
  const rows: Row[] = [{ name: "fig", qty: 2 }, { name: "plum", qty: 5 }];
  return <table><tbody>{rows.map((row) => <tr key={row.name}>{columns.map((c) => <td key={c.key}>{c.render(row)}</td>)}</tr>)}</tbody></table>;
}
