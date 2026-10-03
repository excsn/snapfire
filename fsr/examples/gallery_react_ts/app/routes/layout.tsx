import type { ReactNode } from "react";
import { Link, Picture } from "@snapfire/fsr-client/react";

import logo from "../src/img/logo.svg";

export default function GalleryLayout({ children }: { children: ReactNode }) {
  return (
    <div className="gallery">
      <header className="masthead">
        <Link href="/" className="wordmark">
          <Picture src={logo} alt="Light, a gallery" />
        </Link>
        <nav>
          <Link href="/">Wall</Link>
          <Link href="/about">About</Link>
        </nav>
      </header>
      <main>{children}</main>
      <footer>Four phone photos, stored on their side, shown the right way up.</footer>
    </div>
  );
}
