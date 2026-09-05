import type { Ctx } from "@snapfire/fsr";

export async function load({ session, services }: Ctx) {
  const alerts = await services.fleet.listAlerts();
  return { open: BigInt(alerts.length), watching: BigInt(Object.keys(session.watching).length), density: session.density };
}

/// `fleet/headline` is derived again in the browser, so the server settles on
/// the same string here; a key a component reads and only the browser computes
/// would disagree with the markup React is hydrating.
export const store = ({ data }: { data: { open: bigint; watching: bigint; density: string } }) => ({
  "alerts/open": Number(data.open),
  "fleet/watching": Number(data.watching),
  "fleet/region": "all",
  "ui/density": data.density,
  "fleet/headline": `${Number(data.open) === 0 ? "quiet" : `${Number(data.open)} to look at`}${Number(data.watching) === 0 ? "" : `, watching ${Number(data.watching)}`}`,
});
