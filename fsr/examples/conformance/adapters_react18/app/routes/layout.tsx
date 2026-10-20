import { Island, type Children } from "@snapfire/fsr-authoring/template";
import { Link } from "@snapfire/fsr/components";

import PlaceProbe from "@src/ui/PlaceProbe.vue";

export default function Layout({ children }: { children: Children }) {
  return (
    <div className="shell">
      <header className="bar">
        <Link href="/" className="brand">
          adapters
        </Link>
        <Island>
          <PlaceProbe label="placed by the layout" />
        </Island>
      </header>
      <main className="content">{children}</main>
    </div>
  );
}
