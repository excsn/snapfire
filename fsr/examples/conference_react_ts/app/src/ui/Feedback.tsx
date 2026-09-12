import { useState } from "react";

const PACES = ["too fast", "just right", "too slow"];

export function Feedback({ title }: { title: string }) {
  const [pace, setPace] = useState("");
  return (
    <div className="feedback">
      <h3>How was the pace?</h3>
      {pace === "" ? (
        <div className="pace-buttons">
          {PACES.map((name) => (
            <button key={name} className="btn btn-small" onClick={() => setPace(name)}>
              {name}
            </button>
          ))}
        </div>
      ) : (
        <p className="pace-noted">
          Noted: <strong>{title}</strong> was {pace}.
        </p>
      )}
    </div>
  );
}
