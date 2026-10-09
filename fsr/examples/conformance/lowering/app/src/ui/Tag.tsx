import { use } from "react";

import { Theme } from "@src/theme";

export default function Tag({ loud }: { loud: boolean }) {
  return <span className="tag">{loud ? use(Theme).toUpperCase() : use(Theme)}</span>;
}
