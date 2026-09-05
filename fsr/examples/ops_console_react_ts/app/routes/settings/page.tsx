import type { SettingsProps } from "@generated/client";
import { SettingsPanel } from "@src/ui/SettingsPanel";

export default function SettingsPage({ watched }: SettingsProps) {
  return (
    <div className="page">
      <h1>Settings</h1>
      <p className="lede">Held in your session, so they follow you across pages and a reload.</p>
      <SettingsPanel watched={watched} />
    </div>
  );
}
