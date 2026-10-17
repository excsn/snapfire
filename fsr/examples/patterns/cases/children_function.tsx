function Value({ children }: { children: (v: number) => any }) {
  return <div>{children(42)}</div>;
}
export default function Page() {
  return <Value>{(v) => <i>{v}</i>}</Value>;
}
