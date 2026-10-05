import { Link, type Children } from "@snapfire/fsr-authoring/template";

export default function Layout({ children }: { children: Children }) {
  return (
    <div className="shell">
      <header className="bar">
        <Link href="/" className="brand">
          lowering
        </Link>
        <nav>
          <Link href="/methods">methods</Link>
          <Link href="/patterns">patterns</Link>
          <Link href="/statements">statements</Link>
          <Link href="/made">made</Link>
          <Link href="/regex">regex</Link>
          <Link href="/hooks">hooks</Link>
          <Link href="/vue">vue</Link>
        </nav>
      </header>
      <main className="content">{children}</main>
    </div>
  );
}
