function Card({ title, children }: { title: string; children?: any }) {
  return <section><h2>{title}</h2>{children}</section>;
}
export default function Page() {
  return <Card title="t"><p>body</p></Card>;
}
