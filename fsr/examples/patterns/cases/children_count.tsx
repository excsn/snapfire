import { Children } from "react";
function Grid({ children }: { children?: any }) {
  return <div data-n={Children.count(children)}>{children}</div>;
}
export default function Page() {
  return <Grid><p>a</p><p>b</p>{null}<p>c</p></Grid>;
}
