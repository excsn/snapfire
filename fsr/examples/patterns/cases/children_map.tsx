import { Children } from "react";
function List({ children }: { children?: any }) {
  return <ul>{Children.map(children, (child) => <li>{child}</li>)}</ul>;
}
export default function Page() {
  return <List><b>a</b><b>b</b></List>;
}
