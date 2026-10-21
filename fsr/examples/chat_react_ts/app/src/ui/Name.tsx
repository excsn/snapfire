import { useState } from "react";
import type { FormEvent } from "react";
import { actions } from "@generated/client";

/** Names the visitor, which is what the session holds and what a message is signed with. The input is uncontrolled, so the form works the way a browser expects one to. With `next`, the action sends the visitor on to that page once named. */
export default function Name({ next }: { next: string | null }) {
  const [saved, setSaved] = useState("");
  async function keep(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const form = event.currentTarget;
    const name = String(new FormData(form).get("name") ?? "").trim();
    if (!name) return;
    await actions.$root.name(next ? { name, next } : { name });
    setSaved(name);
  }
  return (
    <form className="name" onSubmit={keep}>
      <label htmlFor="name">Called</label>
      <input id="name" name="name" placeholder="alice" />
      <button type="submit">{saved ? `saved as ${saved}` : "save"}</button>
    </form>
  );
}
