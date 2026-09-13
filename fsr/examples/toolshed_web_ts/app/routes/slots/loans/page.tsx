import { Island } from "@snapfire/fsr-authoring/template";

import type { LayoutLoansProps } from "@generated/client";

export default function Loans({ loans }: LayoutLoansProps) {
  return (
    <div className="panel loans" hx-get="?__fragment=loans" hx-trigger="every 15s" hx-swap="outerHTML">
      <h2>Out right now</h2>
      <Island when="visible" define="@src/elements/time-ago.ts">
        <ul>
          {loans.map((loan) => (
            <li key={loan.tool}>
              <span className="text">{loan.tool}</span>
              <span className="at">
                {loan.who}, <time-ago datetime={loan.back}>back {loan.back}</time-ago>
              </span>
            </li>
          ))}
        </ul>
      </Island>
      <p className="quiet">htmx asks for this slot alone every fifteen seconds.</p>
    </div>
  );
}
