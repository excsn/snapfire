import type { Children } from "@snapfire/fsr/template";
import { Link } from "@snapfire/fsr/components";

export default function Layout({ children }: { children: Children }) {
  return (
    <div className="shell">
      <header className="bar">
        <Link href="/" className="brand">
          patterns
        </Link>
      </header>
      <main className="content">{children}</main>
    </div>
  );
}
