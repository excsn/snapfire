export default function Page() {
  const head = [<li key="h">head</li>];
  const all = [...head, <li key="m">mid</li>].concat([<li key="t">tail</li>]);
  return <ul>{all}</ul>;
}
