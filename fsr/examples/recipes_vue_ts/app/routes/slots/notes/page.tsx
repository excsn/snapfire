import type { LayoutNotesProps } from "@generated/client";

export default function Notes({ notes }: LayoutNotesProps) {
  return (
    <div className="panel notes">
      <h2>On the fridge</h2>
      {notes.length === 0 ? <p className="quiet">Nothing pinned.</p> : null}
      <ul>
        {notes.map((note) => (
          <li key={note.day}>
            <span className="at">{note.day}</span>
            <span className="text">{note.text}</span>
          </li>
        ))}
      </ul>
      <p className="note">Read every time. No cache policy on this method.</p>
    </div>
  );
}
