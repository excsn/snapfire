import type { Ctx } from "@snapfire/fsr";

export async function load({ query }: Ctx) {
  return { denied: query.error === "denied" };
}

export const meta = () => ({ title: "Sign in · Ops console" });
