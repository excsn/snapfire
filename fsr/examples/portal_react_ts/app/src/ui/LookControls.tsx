import { useState } from "react";
import { refresh } from "@snapfire/fsr-client";

import { actions } from "@generated/client";

const themes = [
  { value: "", label: "Page's own" },
  { value: "default", label: "Default" },
  { value: "ocean", label: "Ocean" },
  { value: "forest", label: "Forest" },
];

/** The visitor's theme and light or dark, saved in the session; the page redraws from the server's answer, so a site that forces its theme keeps it. With no saved scheme the browser's preference decides, which only the browser knows, so the button's label follows the `dark` variant. */
export function LookControls({ theme }: { theme: string }) {
  const [picked, setPicked] = useState(theme);

  return (
    <div className="flex items-center gap-2">
      <select
        aria-label="Theme"
        value={picked}
        className="rounded-md border border-line bg-card px-2 py-1 text-[13px] text-ink [[data-sf-theme-forced]_&]:pointer-events-none [[data-sf-theme-forced]_&]:opacity-50"
        onChange={async (event) => {
          setPicked(event.target.value);
          await actions.account.setTheme({ theme: event.target.value });
          await refresh();
        }}
      >
        {themes.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <span className="hidden text-[12px] text-ink-soft [[data-sf-theme-forced]_&]:inline">This site keeps its own theme</span>
      <button
        type="button"
        data-scheme-toggle=""
        aria-label="Switch between light and dark"
        className="pill cursor-pointer hover:border-accent hover:text-accent"
        onClick={async () => {
          const root = document.documentElement;
          const now = root.getAttribute("data-sf-scheme") ?? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
          const next = now === "dark" ? "light" : "dark";
          root.setAttribute("data-sf-scheme", next);
          await actions.account.setScheme({ scheme: next });
        }}
      >
        <span className="dark:hidden">Dark</span>
        <span className="hidden dark:inline">Light</span>
      </button>
    </div>
  );
}
