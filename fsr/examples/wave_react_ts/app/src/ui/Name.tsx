import { useState } from "react";
import type { FormEvent } from "react";
import { actions } from "@generated/client";
import { named } from "@src/ui/wire";

/** Names the reader. Everything else about a wave is durable; this is only who you are while you are here. Naming is what unlocks writing, and `me` reaches the composers through the page, so the document is asked for again rather than revalidated. */
export default function Name({ name }: { name: string }) {
  const [saved, setSaved] = useState(name);
  async function keep(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const form = event.currentTarget;
    const next = String(new FormData(form).get("name") ?? "").trim();
    if (!next) return;
    await actions.$root.name({ name: next });
    setSaved(next);
    named(next);
    // A revalidation patches the page and not the islands under it, so `me`
    // would stay empty in every composer.
    window.location.reload();
  }
  return (
    <form className="name" onSubmit={keep}>
      <input name="name" defaultValue={name} placeholder="who are you" />
      <button type="submit">{saved ? saved : "set"}</button>
    </form>
  );
}
