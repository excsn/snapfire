import { Link, type Children } from "@snapfire/fsr-authoring/template";

export default function Layout({ children }: { children: Children }) {
  return (
    <div className="shell">
      <header className="bar">
        <Link href="/" className="brand">
          adapters
        </Link>
      </header>
      <main className="content">{children}</main>
    </div>
  );
}
