import { questions } from "@src/content";

export async function load() {
  return { questions };
}

export const meta = () => ({ title: "FAQ · The FSR handbook" });
