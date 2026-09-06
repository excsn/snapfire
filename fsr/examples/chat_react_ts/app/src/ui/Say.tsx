import type { FormEvent } from "react";
import { actions } from "@generated/client";

/** The composer. The action keeps the message and the room's topic goes out, so every other page following this room hears it without this one telling them. */
export default function Say({ room }: { room: string }) {
  async function send(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const form = event.currentTarget;
    const body = String(new FormData(form).get("body") ?? "").trim();
    if (!body) return;
    form.reset();
    await actions.room.$id.say({ room, body });
  }
  return (
    <form className="say" onSubmit={send}>
      <input name="body" placeholder={`Say something in ${room}`} />
      <button type="submit">Send</button>
    </form>
  );
}
