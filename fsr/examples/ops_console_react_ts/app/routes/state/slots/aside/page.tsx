import { Island } from "@snapfire/fsr/client/react";

import { Reminder } from "@src/ui/Reminder";

export default function StateAside() {
  return (
    <aside className="state-aside">
      <Island when="visible">
        <Reminder />
      </Island>
    </aside>
  );
}
