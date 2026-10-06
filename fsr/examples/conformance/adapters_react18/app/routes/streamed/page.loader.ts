import type { Ctx } from "@snapfire/fsr";

export async function load(_: Ctx<"/streamed">) {
  return { by: "page" };
}

export const store = ({ data }: { data: { by: string } }) => ({ "repro/owner": data.by });
