import { useContext } from "react";

import { Theme } from "@src/theme";

export default function Label({ name }: { name: string }) {
  const theme = useContext(Theme);
  return (
    <span className={`label ${name}`}>
      {name}:{theme}
    </span>
  );
}
