import { photos } from "@src/content";

export async function load() {
  return { photos };
}

export const meta = () => ({ title: "Four evenings", description: "Four phone photos on a wall, served in every size the wall needs." });
