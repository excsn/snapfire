import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr/components";

import Status from "../src/ui/Status";

export default function TrailsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="trails">
      <header>
        <Link href="/" className="wordmark">
          Trails
        </Link>
        <Status />
      </header>
      <main>{children}</main>
      <footer>Open a trail once with a signal and it is here without one.</footer>
    </div>
  );
}
