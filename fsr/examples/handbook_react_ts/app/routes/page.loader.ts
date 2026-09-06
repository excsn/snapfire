import { cards } from "@src/content";

export async function load() {
  return { cards };
}

export const meta = () => ({ title: "The FSR handbook", description: "A site with no server: every route is a file." });
