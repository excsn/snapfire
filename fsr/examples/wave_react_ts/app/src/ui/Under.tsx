import { useEffect, useState } from "react";
import type { FormEvent, KeyboardEvent } from "react";
import { useStore } from "@snapfire/fsr-client/react";
import { key } from "@snapfire/fsr-client/store";

import { actions } from "@generated/client";
import { join, typing } from "@src/ui/wire";

interface Draft {
  who: string;
  parent: string;
  body: string;
}

/** What sits under one blip and is not kept: whoever else is typing a reply to it, and this reader's own composer. The blips themselves are rendered by the server; this is the part that could not be. A reader with no name gets no composer, and the action refuses one anyway. */
export default function Under({ wave, parent, me, open = false }: { wave: string; parent: string; me: string; open?: boolean }) {
  const [drafts] = useStore(key<Draft[]>("wave/drafts"), []);
  const [writing, setWriting] = useState(open);
  useEffect(() => join(`wave/${wave}`, () => {}), [wave]);

  const ghosts = drafts.filter((draft) => draft.parent === parent && draft.who !== me);

  function chord(event: KeyboardEvent<HTMLInputElement>): void {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") event.currentTarget.form?.requestSubmit();
  }

  async function keep(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const form = event.currentTarget;
    const body = String(new FormData(form).get("body") ?? "").trim();
    if (!body) return;
    form.reset();
    typing(parent, "");
    if (!open) setWriting(false);
    await actions.$root.blip({ wave, parent, body });
  }

  return (
    <div className="under">
      <div className="ghosts">
        {ghosts.map((draft) => (
          <div key={draft.who} className="blip ghost">
            <span className="who">{draft.who}</span>
            <span className="at">typing</span>
            <p className="body">{draft.body}</p>
          </div>
        ))}
      </div>
      {!me ? (
        open ? <p className="nameless">Name yourself at the top to write on this wave.</p> : null
      ) : writing ? (
        <form className="composer" onSubmit={keep}>
          <input name="body" placeholder={parent ? "Reply" : "Add to the wave"} onChange={(e) => typing(parent, e.target.value)} onKeyDown={chord} autoFocus={!open} />
          <button type="submit">Keep</button>
        </form>
      ) : (
        <button className="reply" onClick={() => setWriting(true)}>
          Reply
        </button>
      )}
    </div>
  );
}
