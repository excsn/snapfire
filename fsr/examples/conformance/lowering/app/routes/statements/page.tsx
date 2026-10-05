import { Island } from "@snapfire/fsr-authoring/template";

import Badge from "@src/ui/Badge";
import Chooser from "@src/ui/Chooser";
import Fold from "@src/ui/Fold";
import Shares from "@src/ui/Shares";

interface Item {
  sku: string;
  left: number;
  kind: string;
}

export default function Statements({ total, low, kinds, note, items, pick }: { total: number; low: number; kinds: string[]; note: string; items: Item[]; pick: string }) {
  if (pick === "none") return <p className="empty">nothing picked</p>;
  const chosen = items.find((item) => item.sku === pick) ?? items[0];
  let headline = `${total} in stock`;
  if (low > 0) {
    headline += `, ${low} low`;
  }
  let tone = "calm";
  switch (note) {
    case "reorder":
      tone = "urgent";
      break;
    default:
      tone = "calm";
  }
  return (
    <section className="statements">
      <p className="headline">{headline}</p>
      <p className="tone">{tone}</p>
      <p className="kinds">{kinds.join(", ")}</p>
      <p className="sorted">{kinds.toSorted().join(", ")}</p>
      <p className="chosen">{chosen.sku}</p>
      <ul className="badges">
        {items.map((item) => (
          <li key={item.sku}>
            <Island>
              <Badge left={item.left} kind={item.kind} />
            </Island>
          </li>
        ))}
      </ul>
      <Island>
        <Shares items={items} />
      </Island>
      <Island mode="server">
        <Chooser />
      </Island>
      <Island>
        <Fold title="details">
          <p className="folded">{headline}</p>
        </Fold>
      </Island>
    </section>
  );
}
