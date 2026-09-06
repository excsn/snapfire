import { useState } from "react";
import type { FormEvent } from "react";
import { actions } from "@generated/client";

/** Names the visitor, which is what the session holds and what a message is signed with. The input is uncontrolled, so the form works the way a browser expects one to. */
export default function Name() {
  const [saved, setSaved] = useState("");
  async function keep(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const form = event.currentTarget;
    const name = String(new FormData(form).get("name") ?? "").trim();
    if (!name) return;
    await actions.$root.name({ name });
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
