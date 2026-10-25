import type { Ctx } from "@snapfire/fsr/host";

export async function load({ session }: Ctx) {
  return { theme: session.theme, scheme: session.scheme };
}

export const meta = () => ({ title: "Themes · Acme portal" });
