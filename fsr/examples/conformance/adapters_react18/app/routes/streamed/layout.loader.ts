import type { Ctx } from "@snapfire/fsr";

export async function load(_: Ctx<"/streamed">) {
  return { by: "layout" };
}

export const store = ({ data }: { data: { by: string } }) => ({ "repro/owner": data.by });
