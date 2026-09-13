import { actions } from "@generated/client";
import type { Cell } from "@generated/client";

/**
 * The wave's gadget, in server mode: no module is loaded for it and nothing
 * mounts. A cell posts to the host, the lowered handler calls the action and
 * Rust decides everything a move is, since a handler may not branch and a
 * move is nothing but branches.
 */
export default function Gadget({ wave, cells, turn, won }: { wave: string; cells: Cell[]; turn: string; won: string }) {
  return (
    <div className="gadget">
      <div className="board">
        {cells.map((cell) => (
          <button
            key={`${cell.at}`}
            className={cell.mark === "" ? "cell" : `cell ${cell.mark}`}
            value={`${cell.at}`}
            onClick={(e) => void actions.$root.play({ wave, cell: Number((e.target as HTMLButtonElement).value) })}
          >
            {cell.mark}
          </button>
        ))}
      </div>
      <p className="state">
        {won === "" ? `${turn} to play` : `${won} has it`}
        <button className="again" onClick={() => void actions.$root.reset({ wave })}>
          new board
        </button>
      </p>
    </div>
  );
}
