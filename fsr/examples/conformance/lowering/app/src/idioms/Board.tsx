import { useState } from "react";

function calculateWinner(squares: (string | null)[]) {
  const lines = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [0, 3, 6], [1, 4, 7], [2, 5, 8], [0, 4, 8], [2, 4, 6]];
  for (let i = 0; i < lines.length; i++) {
    const [a, b, c] = lines[i];
    if (squares[a] && squares[a] === squares[b] && squares[a] === squares[c]) {
      return squares[a];
    }
  }
  return null;
}

export default function Board() {
  const [squares, setSquares] = useState<(string | null)[]>(Array(9).fill(null));
  const [xIsNext, setXIsNext] = useState(true);
  const winner = calculateWinner(squares);
  function play(i: number) {
    if (squares[i] || calculateWinner(squares)) {
      return;
    }
    const next = squares.slice();
    next[i] = xIsNext ? "X" : "O";
    setSquares(next);
    setXIsNext(!xIsNext);
  }
  return (
    <div className="board">
      <p className="status">{winner ? "Winner: " + winner : "Next: " + (xIsNext ? "X" : "O")}</p>
      {squares.map((square, i) => (
        <button key={i} className="square" onClick={() => play(i)}>
          {square}
        </button>
      ))}
    </div>
  );
}
