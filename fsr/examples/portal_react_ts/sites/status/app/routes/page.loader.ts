import { systems } from "@src/systems";

export async function load() {
  const down = systems.filter((system) => system.state !== "operational").length;
  return { systems, summary: down === 0 ? "Everything is operational." : `${down} of ${systems.length} systems ${down === 1 ? "needs" : "need"} attention.` };
}

export const meta = () => ({ title: "Status" });
