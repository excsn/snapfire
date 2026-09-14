import { useEffect } from "react";
import type { FormEvent, KeyboardEvent } from "react";
import { useStore } from "@snapfire/fsr-client/react";
import { key } from "@snapfire/fsr-client/store";

import { actions } from "@generated/client";
import { follow } from "@src/ui/follow";
import { join, typing } from "@src/ui/wire";

interface Draft {
  who: string;
  parent: string;
  anchor: string;
  body: string;
}

/** A blip holding one gadget of `kind`, written as the fence the service reads. What was typed is the question a vote asks or a line above a board; a poll starts with two choices to rewrite. */
function gadgetBlip(kind: string, text: string): string {
  const fence = (lines: string[]): string => ["```gadget " + kind, ...lines, "```"].join("\n");
  if (kind === "yesno") return fence([text || "Agreed?"]);
  if (kind === "poll") return fence([text || "Which one?", "One", "The other"]);
  return text ? `${text}\n\n${fence([])}` : fence([]);
}

/** What sits under one blip and is not kept: whoever else is typing a reply there and this reader's own composer. With `anchor` it sits beside that block of the blip instead. The blips themselves are rendered by the server; this is the part that could not be. A reader with no name gets no composer and the action refuses one anyway. A window has one reply open at a time: `wave/replying` names it, so opening one closes the other. Cancel or Escape closes it too. The wave's own composer, `open`, stays open. A blip this reader keeps is brought into view when it lands out of view. Its Gadget menu keeps a blip that is one gadget, what was typed so far its question. */
export default function Under({ wave, parent, anchor = "", me, open = false }: { wave: string; parent: string; anchor?: string; me: string; open?: boolean }) {
  const [drafts] = useStore(key<Draft[]>("wave/drafts"), []);
  const [replying, setReplying] = useStore(key<string>("wave/replying"), "");
  useEffect(() => join(`wave/${wave}`, () => {}), [wave]);

  const here = `${parent}#${anchor}`;
  const writing = open || replying === here;
  const ghosts = drafts.filter((draft) => draft.parent === parent && draft.anchor === anchor && draft.who !== me);

  function start(): void {
    typing(parent, anchor, "");
    setReplying(here);
  }

  function stop(): void {
    typing(parent, anchor, "");
    setReplying("");
  }

  function chord(event: KeyboardEvent<HTMLInputElement>): void {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") event.currentTarget.form?.requestSubmit();
    if (event.key === "Escape" && !open) stop();
  }

  /** The action's revalidation has put the kept blip in the transcript by the time it answers. */
  async function send(form: HTMLFormElement, body: string): Promise<void> {
    form.reset();
    typing(parent, anchor, "");
    if (!open) setReplying("");
    const { kept } = await actions.$root.blip({ wave, parent, anchor, body });
    const landed = document.getElementById(`blip-${kept.id}`);
    if (landed) follow(landed);
  }

  async function keep(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const form = event.currentTarget;
    const body = String(new FormData(form).get("body") ?? "").trim();
    if (!body) return;
    await send(form, body);
  }

  async function add(kind: string, button: HTMLButtonElement): Promise<void> {
    const form = button.form;
    if (!form) return;
    const text = String(new FormData(form).get("body") ?? "").trim();
    button.closest("details")?.removeAttribute("open");
    await send(form, gadgetBlip(kind, text));
  }

  return (
    <div className={anchor ? "under aside" : "under"}>
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
          <input name="body" placeholder={anchor ? "Reply to this" : parent ? "Reply" : "Add to the wave"} onChange={(e) => typing(parent, anchor, e.target.value)} onKeyDown={chord} autoFocus={!open} />
          <details className="add-gadget">
            <summary>Gadget</summary>
            <div>
              <button type="button" onClick={(e) => void add("noughts", e.currentTarget)}>
                Noughts and crosses
              </button>
              <button type="button" onClick={(e) => void add("yesno", e.currentTarget)}>
                Yes / No / Maybe
              </button>
              <button type="button" onClick={(e) => void add("poll", e.currentTarget)}>
                Poll
              </button>
            </div>
          </details>
          <button type="submit">Send</button>
          {open ? null : (
            <button type="button" className="cancel" onClick={stop}>
              Cancel
            </button>
          )}
        </form>
      ) : (
        <button className="reply" onClick={start}>
          {anchor ? "Reply to this" : "Reply"}
        </button>
      )}
    </div>
  );
}
