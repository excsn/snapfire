import { Island } from "@snapfire/fsr-client/react";

import DensityLabel from "@src/planner/DensityLabel";
import Export from "@src/planner/Export";
import Later from "@src/planner/Later";
import PlannerBoundary from "@src/planner/PlannerBoundary";
import PlannerShell from "@src/planner/PlannerShell";
import Reminders from "@src/planner/Reminders";
import Floors from "@src/ui/Floors.vue";

export default function PlannerPage() {
  return (
    <article className="page planner-page">
      <h1>Planner</h1>
      <PlannerShell>
        <PlannerBoundary>
          <Island>
            <DensityLabel place="body" />
          </Island>
          <Reminders />
          <Export />
          <Island>
            <Floors talks={[{ id: "1", title: "Opening", room: "Hall B", starts: "09:00" }]} />
          </Island>
          <div className="planner-gap" />
          <Island when="visible">
            <Later />
          </Island>
        </PlannerBoundary>
      </PlannerShell>
    </article>
  );
}
