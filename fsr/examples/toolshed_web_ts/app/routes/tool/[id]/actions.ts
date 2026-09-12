import { action, fail } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { ReserveTool } from "@schemas/shed";

export const reserve = action(async ({ input, session, services }: ActionCtx<ReserveTool>) => {
  const tools = await services.shed.listTools();
  const known = tools.some((t) => t.id === input.tool_id);
  if (!known) fail("not_found", "there is no tool with that id");
  session.reserved = { ...session.reserved, [input.tool_id]: true };
  return { reserved: Object.keys(session.reserved).length };
});

export const release = action(async ({ input, session }: ActionCtx<ReserveTool>) => {
  if (!session.reserved[input.tool_id]) fail("not_found", "that tool is not reserved");
  const kept = Object.keys(session.reserved)
    .filter((id) => id !== input.tool_id)
    .reduce((held: Record<string, boolean>, id) => ({ ...held, [id]: true }), {});
  session.reserved = kept;
  return { reserved: Object.keys(kept).length };
});
