import type { Ctx } from "@snapfire/fsr/host";
import { t } from "@snapfire/fsr/host";

export async function load({ query }: Ctx) {
  return { denied: query.error === "denied" };
}

export const meta = () => ({ title: t("meta.login") });
