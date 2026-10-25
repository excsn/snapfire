import type { Ctx } from "@snapfire/fsr/host";

export async function load({ session, services }: Ctx) {
  let open = 0n;
  try {
    open = BigInt((await services.fleet.listAlerts()).length);
  } catch {}
  return { open, watching: BigInt(Object.keys(session.watching).length), density: session.density };
}

export const store = ({ data }: { data: { open: bigint; watching: bigint; density: string } }) => ({
  "alerts/open": Number(data.open),
  "fleet/watching": Number(data.watching),
  "fleet/region": "all",
  "ui/density": data.density,
});
