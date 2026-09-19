import { notices } from "@src/content";

export async function load() {
  return { notices };
}

export const meta = () => ({ title: "Noticeboard", description: "What is on the board this week." });
