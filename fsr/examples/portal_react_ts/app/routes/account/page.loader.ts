import type { Ctx } from "@snapfire/fsr";

export async function load({ identity }: Ctx) {
  return { subject: identity?.subject ?? "", role: String(identity?.claims.role ?? "member") };
}

export const meta = () => ({ title: "Account · Acme portal" });
