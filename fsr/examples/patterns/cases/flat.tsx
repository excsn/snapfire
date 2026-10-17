export default function Page() {
  const groups = [[<b key="a">a</b>], [<i key="b">b</i>]];
  return <div>{groups.flat()}</div>;
}
