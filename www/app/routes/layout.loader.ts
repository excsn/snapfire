import type { Ctx } from "@snapfire/fsr";
import { og, themeColor, twitter } from "@snapfire/fsr/head";

export async function load({ now }: Ctx) {
  return {
    bootTimestamp: now,
  };
}

export const store = ({ data }: { data: { bootTimestamp: bigint } }) => ({
  "site/theme": "dark",
  "site/render_time": Number(data.bootTimestamp),
});

export const meta = () => ({
  head: [og("site_name", "SnapFire"), og("type", "website"), twitter("card", "summary_large_image"), themeColor("#0d1117")],
});
