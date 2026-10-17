function Pair() {
  return [<dt key="k">key</dt>, <dd key="v">value</dd>];
}
export default function Page() {
  return <dl><Pair /></dl>;
}
