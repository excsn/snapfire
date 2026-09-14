import { actions } from "@generated/client";
import type { Gadget as Shown } from "@generated/client";

/**
 * A gadget in a blip, in server mode: no module is loaded for it and nothing
 * mounts. A board's cell or a vote's answer posts to the host, the lowered
 * handler calls the action and Rust decides everything the click means, since
 * a handler may not branch. `wave`, `blip` and `block` name the gadget for
 * the action. On a step of playback it is the gadget as it stood, which
 * nobody can use.
 */
export default function Gadget({ wave, blip, block, gadget, live }: { wave: string; blip: string; block: string; gadget: Shown; live: boolean }) {
  return gadget.kind === "noughts" ? (
    <div className={gadget.lit ? "gadget lit" : "gadget"}>
      <div className="board">
        {gadget.cells.map((cell) => (
          <button
            key={`${cell.at}`}
            className={cell.mark === "" ? "cell" : `cell ${cell.mark}`}
            value={`${cell.at}`}
            disabled={!live}
            onClick={(e) => void actions.$root.play({ wave, blip, block, cell: Number((e.target as HTMLButtonElement).value) })}
          >
            {cell.mark}
          </button>
        ))}
      </div>
      <p className="state">
        {gadget.won === "" ? `${gadget.turn} to play` : `${gadget.won} has it`}
        {live ? (
          <button className="again" onClick={() => void actions.$root.reset({ wave, blip, block })}>
            new board
          </button>
        ) : null}
      </p>
    </div>
  ) : (
    <div className={gadget.lit ? "gadget votes lit" : "gadget votes"}>
      {gadget.question === "" ? null : <p className="question">{gadget.question}</p>}
      <ul className="choices">
        {gadget.choices.map((choice) => (
          <li key={choice.answer} className={choice.count > 0 ? "choice chosen" : "choice"}>
            <button value={choice.answer} disabled={!live} onClick={(e) => void actions.$root.vote({ wave, blip, block, answer: (e.target as HTMLButtonElement).value })}>
              {choice.answer}
            </button>
            <span className="count">{`${choice.count}`}</span>
            {choice.who.map((who) => (
              <span key={who} className="voter">
                {who}
              </span>
            ))}
          </li>
        ))}
      </ul>
    </div>
  );
}
