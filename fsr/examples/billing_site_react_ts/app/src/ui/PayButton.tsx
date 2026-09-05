import { useState } from "react";

import { actions } from "@generated/client";

export function PayButton({ id, status }: { id: bigint | number; status: string }) {
  const [now, setNow] = useState(status);
  const [busy, setBusy] = useState(false);
  if (now === "paid") return <p className="paid">Paid.</p>;
  return (
    <button
      className="pay"
      disabled={busy}
      onClick={async () => {
        setBusy(true);
        const result = await actions.invoice.$id.pay({ id: BigInt(id) });
        setNow(result.status);
        setBusy(false);
      }}
    >
      Pay now
    </button>
  );
}
