import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const loans = await services.shed.listLoans();
  return { loans };
}
