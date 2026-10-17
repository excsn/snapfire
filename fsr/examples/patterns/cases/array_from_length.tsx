export default function Page() {
  return <ol>{Array.from({ length: 3 }, (_, i) => <li key={i}>{i + 1}</li>)}</ol>;
}
