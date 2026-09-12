import type { Ctx } from "@snapfire/fsr";

export async function load({ query, session, services }: Ctx) {
  const tools = await services.shed.listTools();
  const shed = await services.shed.getShed();
  const category = query.category ?? "all";
  const shown = category === "all" ? tools : tools.filter((t) => t.category === category);
  return { category, categories: shed.categories, tools: shown, reserved: Object.keys(session.reserved) };
}
