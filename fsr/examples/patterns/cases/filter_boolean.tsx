export default function Page() {
  const on = false;
  return <div>{[on && <b key="a">a</b>, <i key="b">b</i>, null].filter(Boolean)}</div>;
}
