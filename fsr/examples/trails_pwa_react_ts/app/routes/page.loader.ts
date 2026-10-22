import { trails } from "@src/trails";

export async function load() {
  return { trails: trails.map((t) => ({ id: t.id, name: t.name, region: t.region, km: t.km })) };
}

export const meta = () => ({ title: "Trails" });
