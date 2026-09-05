import { Link } from "@snapfire/fsr-client/react";

import type { SettingsProps } from "@generated/client";
import { SettingsPanel } from "@src/ui/SettingsPanel";

export default function SettingsDrawer({ watched }: SettingsProps) {
  return (
    <div className="drawer">
      <h2>Settings</h2>
      <SettingsPanel watched={watched} />
      <p className="drawer-foot">
        <Link href="/settings" full>
          Open as a page
        </Link>
      </p>
    </div>
  );
}
