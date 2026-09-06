import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const rooms = await services.rooms.listRooms();
  return { rooms };
}

export const meta = () => ({ title: "Rooms" });
