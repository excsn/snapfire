import { useEffect, useRef, useState } from "react";
import type { ChangeEvent, FormEvent, KeyboardEvent, MouseEvent } from "react";
import { useStore } from "@snapfire/fsr-client/react";
import { key } from "@snapfire/fsr-client/store";

import { actions } from "@generated/client";
import { follow, reveal } from "@src/ui/follow";
import { join, typing } from "@src/ui/wire";

interface Draft {
  who: string;
  parent: string;
  anchor: string;
  body: string;
}

/** A blip holding one gadget of `kind`, written as the fence the service reads: a poll's question first, then its answers, with `until` on its info string when voting ends at that UTC minute. */
function fence(kind: string, lines: string[], until = ""): string {
  return ["```gadget " + kind + (until ? ` until=${until}` : ""), ...lines, "```"].join("\n");
}

/** What sits under one blip and is not kept: whoever else is typing a reply there and this reader's own composer. With `anchor` it sits beside that block of the blip instead. The blips themselves are rendered by the server; this is the part that could not be. A reader with no name gets no composer and the action refuses one anyway. A window has one reply open at a time: `wave/replying` names it, so opening one closes the other. Cancel or Escape closes it too. The wave's own composer, `open`, stays open. A blip this reader keeps is brought into view when it lands out of view. Its Gadget menu adds a board at once, with what was typed so far as a line above it. Yes / No / Maybe and Poll open one editor in the composer shaped like the vote it makes. The question is its heading and starts as what was typed. Each answer can be renamed or removed and + adds another. Yes / No / Maybe starts with those three answers and Poll with two blank ones. Either is kept as a poll once two answers are written, with the minute voting ends when one is set. The editor is brought into view as it opens and as it grows. Add to wave keeps it and Back returns to the composer. The others read the words as they are typed only while Show what I type is ticked in the composer's options menu, one setting for the page that starts unticked. The options menu and the Gadget menu share a `name`, so opening one closes the other. Until then they are told who is typing and where. Ticking or unticking it sends what the composer holds under the new setting at once. */
export default function Under({ wave, parent, anchor = "", me, open = false }: { wave: string; parent: string; anchor?: string; me: string; open?: boolean }) {
  const [drafts] = useStore(key<Draft[]>("wave/drafts"), []);
  const [replying, setReplying] = useStore(key<string>("wave/replying"), "");
  const [showing, setShowing] = useStore(key<boolean>("wave/showing"), false);
  const [making, setMaking] = useState(false);
  const [question, setQuestion] = useState("");
  const [answers, setAnswers] = useState<string[]>(["", ""]);
  const [until, setUntil] = useState("");
  const form = useRef<HTMLFormElement>(null);
  useEffect(() => join(`wave/${wave}`, () => {}), [wave]);
  useEffect(() => {
    if (making && form.current) reveal(form.current);
  }, [making, answers.length]);
  useEffect(() => {
    const body = form.current?.querySelector<HTMLInputElement>('input[name="body"]')?.value ?? "";
    if (body) typing(parent, anchor, body, showing);
  }, [showing]);

  const here = `${parent}#${anchor}`;
  const writing = open || replying === here;
  const ghosts = drafts.filter((draft) => draft.parent === parent && draft.anchor === anchor && draft.who !== me);
  const ready = !making || answers.filter((answer) => answer.trim() !== "").length >= 2;

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
    if (making) {
      if (!ready) return;
      const written = answers.map((answer) => answer.trim()).filter((answer) => answer !== "");
      setMaking(false);
      const ends = until ? `${new Date(until).toISOString().slice(0, 16)}Z` : "";
      await send(form, fence("poll", [question.trim() || "Which one?", ...written], ends));
      return;
    }
    const body = String(new FormData(form).get("body") ?? "").trim();
    if (!body) return;
    await send(form, body);
  }

  function pick(kind: string, button: HTMLButtonElement): void {
    const form = button.form;
    if (!form) return;
    const text = String(new FormData(form).get("body") ?? "").trim();
    button.closest("details")?.removeAttribute("open");
    if (kind === "noughts") {
      void send(form, text ? `${text}\n\n${fence(kind, [])}` : fence(kind, []));
      return;
    }
    setQuestion(text);
    setAnswers(kind === "yesno" ? ["Yes", "No", "Maybe"] : ["", ""]);
    setUntil("");
    setMaking(true);
  }

  function rename(event: ChangeEvent<HTMLInputElement>): void {
    const at = Number(event.currentTarget.dataset.at);
    const value = event.currentTarget.value;
    setAnswers(answers.map((answer, i) => (i === at ? value : answer)));
  }

  function more(): void {
    setAnswers([...answers, ""]);
  }

  function drop(event: MouseEvent<HTMLButtonElement>): void {
    const at = Number(event.currentTarget.dataset.at);
    setAnswers(answers.filter((_, i) => i !== at));
  }

  return (
    <div className={anchor ? "under aside" : "under"}>
      <div className="ghosts">
        {ghosts.map((draft) => (
          <div key={draft.who} className="blip ghost">
            <span className="who">{draft.who}</span>
            <span className="at">{draft.body ? "typing" : "is typing"}</span>
            {draft.body ? <p className="body">{draft.body}</p> : null}
          </div>
        ))}
      </div>
      {!me ? (
        open ? <p className="nameless">Name yourself at the top to write on this wave.</p> : null
      ) : writing ? (
        <form ref={form} className={making ? "composer making" : "composer"} onSubmit={keep}>
          <input name="body" hidden={making} placeholder={anchor ? "Reply to this" : parent ? "Reply" : "Add to the wave"} onChange={(e) => typing(parent, anchor, e.target.value, showing)} onKeyDown={chord} autoFocus={!open} />
          {making ? (
            <div className="gadget votes gadget-editor">
              <input className="question" aria-label="question" placeholder="Which one?" value={question} onChange={(e) => setQuestion(e.target.value)} autoFocus />
              <ul className="choices">
                {answers.map((answer, i) => (
                  <li key={i} className="choice">
                    <input className="answer" aria-label={`answer ${i + 1}`} placeholder="an answer" value={answer} data-at={i} onChange={rename} />
                    <button type="button" className="drop" title="remove this answer" aria-label="remove this answer" data-at={i} onClick={drop} disabled={answers.length <= 2}>
                      ✕
                    </button>
                  </li>
                ))}
                <li className="choice">
                  <button type="button" className="more" title="add an answer" aria-label="add an answer" onClick={more}>
                    +
                  </button>
                </li>
              </ul>
              <label className="until">
                Voting ends
                <input type="datetime-local" value={until} onChange={(e) => setUntil(e.target.value)} />
              </label>
            </div>
          ) : (
            <>
              <details className="options" name="composer-menu">
                <summary aria-label="Options" title="Options">
                  <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor" aria-hidden="true">
                    <circle cx="3" cy="8" r="1.5" />
                    <circle cx="8" cy="8" r="1.5" />
                    <circle cx="13" cy="8" r="1.5" />
                  </svg>
                </summary>
                <div>
                  <label className="showing">
                    <input type="checkbox" checked={showing} onChange={(e) => setShowing(e.target.checked)} />
                    <span className="tick" aria-hidden="true">
                      {showing ? "✓" : ""}
                    </span>
                    <span className="name">Show what I type</span>
                    <span className="what">Others read your words as you type</span>
                  </label>
                </div>
              </details>
              <details className="add-gadget" name="composer-menu">
                <summary aria-label="Gadget" title="Gadget">
                  <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth={2} strokeLinejoin="round" aria-hidden="true">
                    <path d="M3.5 7.5H8a2.5 2.5 0 1 1 4 0h4.5V12a2.5 2.5 0 1 1 0 4v4.5h-13Z" />
                  </svg>
                </summary>
                <div>
                  <button type="button" onClick={(e) => pick("noughts", e.currentTarget)}>
                    Noughts and crosses
                  </button>
                  <button type="button" onClick={(e) => pick("yesno", e.currentTarget)}>
                    Yes / No / Maybe
                  </button>
                  <button type="button" onClick={(e) => pick("poll", e.currentTarget)}>
                    Poll
                  </button>
                </div>
              </details>
            </>
          )}
          {making ? (
            <button type="submit" disabled={!ready}>
              Add to wave
            </button>
          ) : (
            <button type="submit" className="send" aria-label="Send" title="Send">
              <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M21 3 3 10.5l7.5 3L13.5 21Z" />
                <path d="M21 3 10.5 13.5" />
              </svg>
            </button>
          )}
          {making ? (
            <button type="button" className="back" onClick={() => setMaking(false)}>
              Back
            </button>
          ) : open ? null : (
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
