import type { Ctx } from "@snapfire/fsr";
import { t } from "@snapfire/fsr-client/std";

export async function load({ query }: Ctx) {
  return { denied: query.error === "denied" };
}

export const meta = () => ({ title: t("meta.login") });
