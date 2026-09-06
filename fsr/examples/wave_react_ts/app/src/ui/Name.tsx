import { useState } from "react";
import type { FormEvent } from "react";
import { actions } from "@generated/client";

/** Names the reader. Everything else about a wave is durable; this is only who you are while you are here. */
export default function Name({ name }: { name: string }) {
  const [saved, setSaved] = useState(name);
  async function keep(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const form = event.currentTarget;
    const next = String(new FormData(form).get("name") ?? "").trim();
    if (!next) return;
    await actions.$root.name({ name: next });
    setSaved(next);
  }
  return (
    <form className="name" onSubmit={keep}>
      <input name="name" defaultValue={name} placeholder="who are you" />
      <button type="submit">{saved ? saved : "set"}</button>
    </form>
  );
}
