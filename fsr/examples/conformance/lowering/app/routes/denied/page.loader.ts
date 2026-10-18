import { fail, type Ctx } from "@snapfire/fsr";

export async function load({ query }: Ctx<"/denied">) {
  if (query.as === "forbidden") fail("forbidden", "this shelf is staff only");
  if (query.as === "busy") fail("too_many_requests", "too many visits, try again shortly");
  return { shelf: "open" };
}
