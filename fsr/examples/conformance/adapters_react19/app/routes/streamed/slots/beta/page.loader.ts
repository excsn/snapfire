import type { Ctx } from "@snapfire/fsr";

export async function load(_: Ctx<"/streamed">) {
  return { by: "beta" };
}

export const store = ({ data }: { data: { by: string } }) => ({ "repro/slot": data.by });
