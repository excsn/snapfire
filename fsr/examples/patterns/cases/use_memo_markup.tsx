import { useMemo } from "react";
export default function Page() {
  const items = ["x", "y"];
  const list = useMemo(() => items.map((i) => <li key={i}>{i}</li>), [items]);
  return <ul>{list}</ul>;
}
