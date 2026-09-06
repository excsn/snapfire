import type { ReactNode } from "react";
import { Island } from "@snapfire/fsr-client/react";

import Live from "@src/ui/Live";

export default function BoardLayout({ children, weather, gates }: { children: ReactNode; weather: ReactNode; gates: ReactNode }) {
  return (
    <div className="board">
      <header>
        <h1>Arrivals</h1>
        <Island when="load">
          <Live topic="board" />
        </Island>
        <p className="strap">Three panels, three services, one document. The board is here before the field reports.</p>
      </header>
      <div className="columns">
        <section className="main">{children}</section>
        <aside className="side">
          {weather}
          {gates}
        </aside>
      </div>
    </div>
  );
}
