import { isValidElement } from "react";
function Label({ value }: { value: any }) {
  return <p>{isValidElement(value) ? value : <span>{value}</span>}</p>;
}
export default function Page() {
  return <div><Label value="text" /><Label value={<b>bold</b>} /></div>;
}
