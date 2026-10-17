import { Fragment } from "react";
export default function Page() {
  const terms = [["a", "1"], ["b", "2"]];
  return <dl>{terms.map(([k, v]) => <Fragment key={k}><dt>{k}</dt><dd>{v}</dd></Fragment>)}</dl>;
}
