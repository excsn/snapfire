import { useState } from "react";

import { actions } from "@generated/client";

export function PayButton({ id, status }: { id: bigint | number; status: string }) {
  const [now, setNow] = useState(status);
  const [busy, setBusy] = useState(false);
  if (now === "paid") return <p className="font-semibold text-emerald-600">Paid.</p>;
  return (
    <button
      className="cursor-pointer rounded-md bg-accent px-3.5 py-1.5 text-white hover:opacity-90 disabled:opacity-60"
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
