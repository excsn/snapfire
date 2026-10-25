import { after } from "@snapfire/fsr/host";
import type { Ctx } from "@snapfire/fsr/host";

/** A short link a pager can carry: `/go/3` opens agent 3 for good. The visit is counted once the redirect is sent, so the pager's tap never waits on it. */
export function GET({ params, services }: Ctx<"/go/{id}">) {
  after(async () => {
    await services.fleet.recordVisit({ id: BigInt(params.id) });
  });
  return Response.redirect(`/agents/${params.id}`, 308);
}
