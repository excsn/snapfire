import { Link } from "@snapfire/fsr-authoring/template";

import type { ToolIdProps } from "@generated/client";

export default function ToolPage({ tool, sameCategory, reserved, days, csrf_token }: ToolIdProps & { csrf_token?: string }) {
  const verb = reserved ? "release" : "reserve";
  return (
    <article className="page tool">
      <p className="when">
        {tool.category} · {tool.keeper}'s · £{tool.deposit} deposit · {tool.days} days at a time
      </p>
      <h2>{tool.name}</h2>
      <p className="tool-note">{tool.note}</p>
      <form className="reserve" method="post" action={`/_sf/action/tool.$id.${verb}`} hx-post={`/_sf/action/tool.$id.${verb}?__fragment`} hx-target="closest .page" hx-swap="outerHTML">
        <input type="hidden" name="_csrf" value={csrf_token ?? ""} />
        <input type="hidden" name="tool_id" value={tool.id} />
        <loan-planner name={reserved ? false : "days"} deposit={tool.deposit} disabled={reserved}>
          <template shadowrootmode="open">
            <style>{":host { display: block; margin: 0 0 16px; padding: 14px 16px; border: 1px solid #e3e5ea; border-radius: 8px; background: #fff; } :host([disabled]) { background: #f7f8fa; } label { display: flex; align-items: center; gap: 10px; } input { flex: 1; } input:disabled { cursor: default; } output { min-width: 5em; font-weight: 600; } p { margin: 8px 0 0; color: #6b7280; font-size: 13px; }"}</style>
            <label>
              {reserved ? "Borrowed for" : "Borrow for"}
              <input type="range" name="days" min="1" max={`${tool.days}`} value={`${days}`} disabled={reserved} />
              <output>{days} days</output>
            </label>
            <p>
              Back <b data-back>in {days} days</b>, £{tool.deposit} {reserved ? "is held" : "would be held"} until then.
            </p>
          </template>
        </loan-planner>
        <div className="row">
          <button type="submit" className="btn">
            {reserved ? "Let it go" : "Reserve it"}
          </button>
          {reserved ? <span className="kept">Reserved for you</span> : <span className="quiet">Nobody has it reserved.</span>}
        </div>
      </form>
      <section className="same-category">
        <h3>Also on the {tool.category} shelf</h3>
        {sameCategory.length === 0 ? (
          <p className="quiet">Nothing else yet.</p>
        ) : (
          <ul>
            {sameCategory.map((other) => (
              <li key={other.id}>
                <Link href={`/tool/${other.id}`}>{other.name}</Link>
                <span className="by">{other.keeper}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </article>
  );
}
