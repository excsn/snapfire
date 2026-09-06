import type { ReactNode } from "react";
import { Link } from "@snapfire/fsr-client/react";

export default function ChatLayout({ children, name }: { children: ReactNode; name?: string }) {
  return (
    <div className="chat">
      <header>
        <Link href="/" className="wordmark">
          Rooms
        </Link>
        <span className="who">{name ? `you are ${name}` : "not named yet"}</span>
      </header>
      {children}
    </div>
  );
}
