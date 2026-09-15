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
        <loan-planner name={reserved ? false : "days"} deposit={tool.deposit} disabled={reserved} max={tool.days} days={days} />
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
