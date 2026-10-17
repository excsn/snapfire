type Node = { name: string; kids: Node[] };
function Tree({ node }: { node: Node }) {
  return <li>{node.name}{node.kids.length > 0 ? <ul>{node.kids.map((k) => <Tree key={k.name} node={k} />)}</ul> : null}</li>;
}
export default function Page() {
  const root: Node = { name: "root", kids: [{ name: "a", kids: [{ name: "a1", kids: [] }] }, { name: "b", kids: [] }] };
  return <ul><Tree node={root} /></ul>;
}
