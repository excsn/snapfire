import type { ReactNode } from "react";

export default function BoardLayout({ children, weather, gates }: { children: ReactNode; weather: ReactNode; gates: ReactNode }) {
  return (
    <div className="board">
      <header>
        <h1>Arrivals</h1>
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
