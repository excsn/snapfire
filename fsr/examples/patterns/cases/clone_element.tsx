import { Children, cloneElement } from "react";
function Marked({ children }: { children?: any }) {
  return <div>{Children.map(children, (child: any) => cloneElement(child, { className: "marked" }))}</div>;
}
export default function Page() {
  return <Marked><p>a</p><p>b</p></Marked>;
}
