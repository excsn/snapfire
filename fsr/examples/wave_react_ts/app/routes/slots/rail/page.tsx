import type { LayoutRailProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

const views = [
  { id: "inbox", label: "Inbox" },
  { id: "active", label: "Active" },
  { id: "mine", label: "By me" },
];

/** The navigation pane. Each view is the page you are on with a different query, so choosing one keeps the wave open beside it. */
export default function Rail({ view, path }: LayoutRailProps) {
  return (
    <nav className="rail">
      <ul>
        {views.map((each) => (
          <li key={each.id}>
            <Link href={`${path}?view=${each.id}`} className={each.id === view ? "view on" : "view"}>
              {each.label}
            </Link>
          </li>
        ))}
      </ul>
    </nav>
  );
}
