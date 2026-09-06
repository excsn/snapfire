import type { LayoutGatesProps } from "@generated/client";

export default function Gates({ changes }: LayoutGatesProps) {
  return (
    <div className="panel gates">
      <h2>Gate changes</h2>
      {changes.length === 0 ? <p className="quiet">None this hour.</p> : null}
      <ul>
        {changes.map((change) => (
          <li key={change.flight}>
            <span className="flight">{change.flight}</span>
            <span className="was">{change.was}</span>
            <span className="now">{change.now}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
