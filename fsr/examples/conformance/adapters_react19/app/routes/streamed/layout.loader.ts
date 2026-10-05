import type { Ctx } from "@snapfire/fsr";

export async function load({ query }: Ctx<"/streamed">) {
  return { by: query.by ?? "layout" };
}

export const store = ({ data }: { data: { by: string } }) => ({ "repro/owner": data.by });
