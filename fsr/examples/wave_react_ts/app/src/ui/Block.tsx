import { useEffect, useState } from "react";
import type { FormEvent, KeyboardEvent, ReactNode } from "react";
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

/** One block of a blip, rewritten on its own. Its markup is the server's: the children this island is placed with. It shows as it stands until someone takes the block. Every other window then watches that person's words in its place and theirs is a form holding the block's markdown. Another block of the blip stays free for someone else; a rewrite of the whole blip holds every block of it. Saving the block as nothing removes it and saving it as several blocks splits it, the first keeping its id. */
export default function Block({ wave, blip, block, text, me, children }: { wave: string; blip: string; block: string; text: string; me: string; children?: ReactNode }) {
  const [edits] = useStore(key<Edit[]>("wave/edits"), []);
  const [mine, setMine] = useState(false);
  const theirs = edits.filter((edit) => edit.blip === blip && edit.block === block && edit.who !== me);
  const whole = edits.some((edit) => edit.blip === blip && edit.block === "" && edit.who !== me);

  useEffect(() => {
    if (theirs.length > 0 && mine) setMine(false);
  }, [theirs.length, mine]);

  function take(): void {
    hold(blip, block);
    setMine(true);
  }

  function drop(): void {
    release(blip, block);
    setMine(false);
  }

  function chord(event: KeyboardEvent<HTMLTextAreaElement>): void {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") event.currentTarget.form?.requestSubmit();
  }

  async function keep(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    const body = String(new FormData(event.currentTarget).get("body") ?? "").trim();
    setMine(false);
    await actions.$root.amend({ wave, blip, block, body });
  }

  return theirs.length > 0 ? (
    <div className="block-held">
      <p className="body">{theirs[0].body}</p>
      <span className="holder">{theirs[0].who} is rewriting this</span>
    </div>
  ) : mine ? (
    <form className="rewrite" onSubmit={keep}>
      <textarea name="body" defaultValue={text} rows={3} onChange={(e) => rewriting(blip, block, e.target.value)} onKeyDown={chord} autoFocus />
      <div className="rewrite-buttons">
        <button type="submit">Save</button>
        <button type="button" className="cancel" onClick={drop}>
          Cancel
        </button>
      </div>
    </form>
  ) : (
    <div className="block-live">
      {children}
      {me && !whole ? (
        <button className="edit-block" onClick={take}>
          Edit
        </button>
      ) : null}
    </div>
  );
}
