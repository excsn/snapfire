import { Link } from "@snapfire/fsr/components";

import type { ReservedProps } from "@generated/client";

export default function ReservedPage({ reserved, deposit, csp_nonce }: ReservedProps) {
  return (
    <section className="page reserved-page">
      <h2>Reserved</h2>
      {reserved.length === 0 ? (
        <p className="quiet">
          Nothing reserved yet. Open <Link href="/tool/3">a tool</Link> and reserve it.
        </p>
      ) : (
        <p className="strap">£{deposit} in deposits, all in.</p>
      )}
      <ol className="tool-list">
        {reserved.map((tool) => (
          <li key={tool.id} className="tool-row">
            <span className="what">
              <Link href={`/tool/${tool.id}`} className="tool-title">
                {tool.name}
              </Link>
              <span className="by">
                {tool.keeper}'s, {tool.days} {Number(tool.days) === 1 ? "day" : "days"}
              </span>
            </span>
            <span className={`category category-${tool.category.toLowerCase()}`}>{tool.category}</span>
          </li>
        ))}
      </ol>
      <p className="note">This page reads the session. Nothing about it is cached.</p>
      <p className="note" id="reserved-opened"></p>
      <script nonce={csp_nonce}>{"document.getElementById('reserved-opened').textContent = 'Opened at ' + new Date().toLocaleTimeString();"}</script>
    </section>
  );
}
