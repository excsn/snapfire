import { useEffect, type ReactNode } from "react";
import { Link, Mount, Picture, useLocale, useStore } from "@snapfire/fsr-client/react";

import { rendered, unmounted } from "@src/probes";
import { probeCount } from "@src/store";

export default function ReactProbe({ label, nest, children }: { label: string; nest: string[]; children?: ReactNode }) {
  const [count, setCount] = useStore(probeCount, 0);
  const locale = useLocale();
  useEffect(() => {
    rendered("react");
  });
  useEffect(() => () => unmounted("react"), []);
  return (
    <section className="probe" data-owner="react">
      <h3 className="label">{label}</h3>
      <p className="count">{count}</p>
      <button className="add" onClick={() => setCount(count + 1)}>
        add
      </button>
      <p className="locale">{locale}</p>
      <Link href="/next" className="next">
        next
      </Link>
      <Picture src="/pictures/probe.png" alt="probe" width={4} height={4} />
      <div className="children">{children}</div>
      {nest.map((module) => (
        <Mount key={module} module={module} props={{ label: `${label} nested`, nest: [] }} />
      ))}
    </section>
  );
}
