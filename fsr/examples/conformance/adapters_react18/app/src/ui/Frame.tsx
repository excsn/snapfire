import { useState, type ReactNode } from "react";

export default function Frame({ children }: { children?: ReactNode }) {
  const [n, setN] = useState(0);
  return (
    <section className="frame" data-frame="react">
      <button className="tick" onClick={() => setN(n + 1)}>
        {n}
      </button>
      {children}
    </section>
  );
}
