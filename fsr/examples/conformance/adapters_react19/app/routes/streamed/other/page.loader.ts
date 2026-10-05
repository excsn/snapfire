import type { Ctx } from "@snapfire/fsr";

export async function load(_: Ctx<"/streamed/other">) {
  return { by: "other" };
}

export const store = ({ data }: { data: { by: string } }) => ({ "repro/owner": data.by });
