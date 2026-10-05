import type { Ctx } from "@snapfire/fsr";

export async function load({ params, query, path, locale, identity, session, services }: Ctx<"/ctx/{id}">) {
  const stamped = await services.probe.getStamp();
  return { id: params.id, q: query.q ?? "", path, locale, subject: identity?.subject ?? "", note: session.note, stamp: stamped.stamp };
}
