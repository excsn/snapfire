export default function Page() {
  const terms = [["a", "1"], ["b", "2"]];
  return <dl>{terms.map(([k, v]) => [<dt key={k + "t"}>{k}</dt>, <dd key={k + "d"}>{v}</dd>])}</dl>;
}
