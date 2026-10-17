import { Children } from "react";
function FirstTwo({ children }: { children?: any }) {
  return <div>{Children.toArray(children).slice(0, 2)}</div>;
}
export default function Page() {
  return <FirstTwo><i>1</i><i>2</i><i>3</i></FirstTwo>;
}
