function price(n: number) {
  return <span className="price">{n.toFixed(2)}</span>;
}
export default function Page() {
  return <p>Now {price(3)}</p>;
}
