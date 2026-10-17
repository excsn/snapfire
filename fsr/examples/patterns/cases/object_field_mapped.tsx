function Faq({ items }: { items: { id: string; q: string; body: any }[] }) {
  return <div>{items.map((item) => <details key={item.id}><summary>{item.q}</summary>{item.body}</details>)}</div>;
}
export default function Page() {
  const data = [{ id: "1", q: "Why?", a: "Because" }, { id: "2", q: "How?", a: "Like so" }];
  return <Faq items={data.map((d) => ({ id: d.id, q: d.q, body: <p>{d.a}</p> }))} />;
}
