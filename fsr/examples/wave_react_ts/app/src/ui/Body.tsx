import { useEffect, useState } from "react";
import type { FormEvent, KeyboardEvent } from "react";
import { useStore } from "@snapfire/fsr-client/react";
import { key } from "@snapfire/fsr-client/store";

import { actions } from "@generated/client";
import { hold, release, rewriting } from "@src/ui/wire";

interface Edit {
  blip: string;
  block: string;
  who: string;
  body: string;
}

/** What is live about one blip as a whole: someone else's rewrite of all of it as they type, this reader's own and the button that starts one. The text itself is the server's, rendered beside this as the blip's blocks. Each block can be rewritten on its own through its `Block`. This is the source editor: it holds the whole blip, so it waits while anyone holds a block of it and every block waits while it is held. The service diffs a save into the blip's blocks, so a block kept or edited keeps its id and the replies anchored to it. */
export default function Body({ wave, blip, text, edited, editors, me }: { wave: string; blip: string; text: string; edited: string; editors: string[]; me: string }) {
  const [edits] = useStore(key<Edit[]>("wave/edits"), []);
  const [mine, setMine] = useState(false);
  const theirs = edits.filter((edit) => edit.blip === blip && edit.block === "" && edit.who !== me);
  const busy = edits.some((edit) => edit.blip === blip && edit.block !== "" && edit.who !== me);

  useEffect(() => {
    if (theirs.length > 0 && mine) setMine(false);
  }, [theirs.length, mine]);

  function take(): void {
    hold(blip, "");
    setMine(true);
  }

  function drop(): void {
    release(blip, "");
    setMine(false);
  }

  function chord(event: KeyboardEvent<HTMLTextAreaElement>): void {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") event.currentTarget.form?.requestSubmit();
  }

  async function keep(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const body = String(new FormData(event.currentTarget).get("body") ?? "").trim();
    if (!body) return;
    setMine(false);
    await actions.$root.amend({ wave, blip, block: "", body });
  }

  return theirs.length > 0 ? (
    <div className="body-held">
      <p className="body">{theirs[0].body}</p>
      <span className="holder">{theirs[0].who} is rewriting this</span>
    </div>
  ) : mine ? (
    <form className="rewrite" onSubmit={keep}>
      <textarea name="body" defaultValue={text} rows={3} onChange={(e) => rewriting(blip, "", e.target.value)} onKeyDown={chord} autoFocus />
      <div className="rewrite-buttons">
        <button type="submit">Save</button>
        <button type="button" className="cancel" onClick={drop}>
          Cancel
        </button>
      </div>
    </form>
  ) : (
    <div className="body-foot">
      {editors.length > 0 ? (
        <ul className="editors" title="everyone who has rewritten this blip">
          {editors.map((who) => (
            <li key={who} className="editor">
              {who}
            </li>
          ))}
        </ul>
      ) : null}
      {edited ? <span className="edited">edited {edited}</span> : null}
      {me && !busy ? (
        <button className="take" onClick={take}>
          Edit
        </button>
      ) : null}
    </div>
  );
}
