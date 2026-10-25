import { file } from "@snapfire/fsr/host";
import type { Ctx } from "@snapfire/fsr/host";

export async function load({ query }: Ctx<"/">) {
  const notes = file.list("notes");
  const path = query.path ?? `notes/${notes[0]}`;
  try {
    return { notes, path, text: file.read(path), refused: "" };
  } catch (e) {
    return { notes, path, text: "", refused: (e as Error).message };
  }
}

export const meta = () => ({
  title: "Notes",
  description: "Notes read from disk, opened with the system's file dialog in a desktop window.",
});
