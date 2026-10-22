import type { Ctx } from "@snapfire/fsr";

/** A short link a pager can carry: `/go/3` opens agent 3 for good. */
export function GET({ params }: Ctx<"/go/{id}">) {
  return Response.redirect(`/agents/${params.id}`, 308);
}
