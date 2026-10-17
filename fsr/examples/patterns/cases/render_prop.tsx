function List({ items, render }: { items: string[]; render: (item: string, i: number) => any }) {
  return <ul>{items.map((item, i) => <li key={item}>{render(item, i)}</li>)}</ul>;
}
export default function Page() {
  return <List items={["a", "b"]} render={(item, i) => <b>{i}:{item}</b>} />;
}
