import { photographer } from "@src/content";

export async function load() {
  return { photographer };
}

export const meta = () => ({ title: "About · Four evenings" });
