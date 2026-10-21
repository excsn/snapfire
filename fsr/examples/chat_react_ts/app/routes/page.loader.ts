import type { Ctx } from "@snapfire/fsr";

export async function load({ services, query }: Ctx) {
  const rooms = await services.rooms.listRooms();
  return { rooms, next: query.next ?? null };
}

export const meta = () => ({ title: "Rooms" });
