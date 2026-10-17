export default function Page() {
  const words = ["one", "two", "three"];
  const items = words.reduce((acc: any[], w) => [...acc, <li key={w}>{w}</li>], []);
  return <ol>{items}</ol>;
}
