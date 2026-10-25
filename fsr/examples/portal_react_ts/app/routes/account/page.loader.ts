import type { Ctx } from "@snapfire/fsr/host";

export async function load({ identity, session }: Ctx) {
  return { subject: identity?.subject ?? "", role: String(identity?.claims.role ?? "member"), theme: session.theme, scheme: session.scheme };
}

export const meta = () => ({ title: "Account · Acme portal" });
