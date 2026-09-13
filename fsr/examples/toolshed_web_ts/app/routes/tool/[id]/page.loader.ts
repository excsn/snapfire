import { fail } from "@snapfire/fsr";
import type { Ctx, DataOf, MetaCtx } from "@snapfire/fsr";

export async function load({ params, session, services }: Ctx<"/tool/{id}">) {
  const tools = await services.shed.listTools();
  const matches = tools.filter((t) => t.id === params.id);
  if (matches.length === 0) fail("not_found", "there is no tool with that id");
  const tool = matches[0];
  const sameCategory = tools.filter((t) => t.category === tool.category && t.id !== tool.id);
  const held = session.reserved[params.id] ?? 0;
  return { tool, sameCategory, reserved: held > 0, days: held > 0 ? held : Number(tool.days) };
}

export const meta = ({ data }: MetaCtx<DataOf<typeof load>>) => ({
  title: `${data.tool.name} · The Shed`,
  description: `${data.tool.keeper}'s, ${data.tool.days} days at a time`,
});
