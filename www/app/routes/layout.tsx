import type { ReactNode } from "react";
import { Link, useStore } from "@snapfire/fsr-client/react";
import { theme } from "@src/store";

export default function RootLayout({ children }: { children: ReactNode }) {
  const [currentTheme, setTheme] = useStore(theme, "dark");

  return (
    <div className={`app-container theme-${currentTheme}`}>
      <header className="navbar">
        <Link href="/" className="brand">
          <img className="brand-mark" src="/static/icons/snapfire-mark.png" alt="" width={128} height={128} />
          <span className="brand-text">SnapFire</span>
        </Link>
        <nav className="nav-links">
          <Link href="/fsr" className="nav-primary">
            FSR
          </Link>
          <Link href="/fsr/docs">Guide</Link>
          <Link href="/fsr/benches">Benchmarks</Link>
          <span className="nav-rule" />
          <Link href="/compiler">Compiler</Link>
          <Link href="/snapfire">snapfire</Link>
          <a href="https://github.com/excsn/snapfire" target="_blank" rel="noreferrer">
            GitHub
          </a>
          <button
            className="theme-toggle"
            onClick={() => setTheme(currentTheme === "dark" ? "light" : "dark")}
            aria-label="Toggle theme"
          >
            {currentTheme === "dark" ? "☀️" : "🌙"}
          </button>
        </nav>
      </header>

      <main className="content">{children}</main>

      <footer className="site-footer">
        <p>
          Built with <strong>SnapFire FSR</strong>. Pure Rust runtime, zero Node.js on the server.
        </p>
      </footer>
    </div>
  );
}
