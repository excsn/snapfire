import { action, fail } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { ReleaseTool, ReserveTool } from "@schemas/shed";

export const reserve = action(async ({ input, session, services }: ActionCtx<ReserveTool>) => {
  const tools = await services.shed.listTools();
  const matches = tools.filter((t) => t.id === input.tool_id);
  if (matches.length === 0) fail("not_found", "there is no tool with that id");
  const cap = Number(matches[0].days);
  const days = Math.min(Math.max(Math.round(Number(input.days ?? cap)), 1), cap);
  session.reserved = { ...session.reserved, [input.tool_id]: days };
  return { reserved: Object.keys(session.reserved).length, days };
});

export const release = action(async ({ input, session }: ActionCtx<ReleaseTool>) => {
  if (!session.reserved[input.tool_id]) fail("not_found", "that tool is not reserved");
  const kept = Object.keys(session.reserved)
    .filter((id) => id !== input.tool_id)
    .reduce((held: Record<string, number>, id) => ({ ...held, [id]: session.reserved[id] }), {});
  session.reserved = kept;
  return { reserved: Object.keys(kept).length };
});
