import { useEffect, useState } from "react";

/** The visitor's own clock and time zone, which only the browser knows, so this renders there alone. */
export function LocalTime() {
  const [now, setNow] = useState("");
  const zone = typeof Intl === "undefined" ? "" : Intl.DateTimeFormat().resolvedOptions().timeZone;
  useEffect(() => {
    const tick = () => setNow(new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }));
    tick();
    const id = setInterval(tick, 30000);
    return () => clearInterval(id);
  }, []);
  return (
    <span className="shrink-0 rounded px-1.5 py-0.5 font-mono whitespace-nowrap text-[12px] tracking-wider text-ink-soft ring-1 ring-line" title={zone}>
      {now || "--:--"}
    </span>
  );
}
