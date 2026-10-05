import { useStore } from "@snapfire/fsr-client/react";

import { probeCount } from "@src/store";

export default function ServerProbe() {
  const [count, setCount] = useStore(probeCount, 0);
  return (
    <section className="probe" data-owner="server">
      <h3 className="label">server</h3>
      <p className="count">{count}</p>
      <button className="add" onClick={() => setCount(count + 1)}>
        add
      </button>
    </section>
  );
}
