import { fail } from "@snapfire/fsr";
import type { Ctx } from "@snapfire/fsr";

export async function load({ params, session, services }: Ctx<"/tool/{id}">) {
  const tools = await services.shed.listTools();
  const matches = tools.filter((t) => t.id === params.id);
  if (matches.length === 0) fail("not_found", "there is no tool with that id");
  const tool = matches[0];
  const sameCategory = tools.filter((t) => t.category === tool.category && t.id !== tool.id);
  return { tool, sameCategory, reserved: session.reserved[params.id] ?? false };
}

export const meta = ({ data }: { data: { tool: { name: string; keeper: string; days: number } } }) => ({
  title: `${data.tool.name} · The Shed`,
  description: `${data.tool.keeper}'s, ${data.tool.days} days at a time`,
});
