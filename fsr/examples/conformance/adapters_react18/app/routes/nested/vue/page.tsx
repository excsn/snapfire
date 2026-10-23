import { Island } from "@snapfire/fsr-authoring/template";

import Leftovers from "@src/nested/Leftovers.vue";
import Pantry from "@src/nested/Pantry.vue";
import PlanBoundary from "@src/nested/PlanBoundary.vue";
import PlanShell from "@src/nested/PlanShell.vue";
import Servings from "@src/nested/Servings.vue";
import ShoppingList from "@src/nested/ShoppingList.vue";

export default function PlanPage() {
  return (
    <section className="page plan-page">
      <h2>Plan the week</h2>
      <Island>
        <PlanShell>
          <Island>
            <PlanBoundary>
              <Island>
                <Servings />
              </Island>
              <Island>
                <ShoppingList />
              </Island>
              <Island>
                <Pantry />
              </Island>
              <div className="plan-gap" />
              <Island when="visible">
                <Leftovers />
              </Island>
            </PlanBoundary>
          </Island>
        </PlanShell>
      </Island>
    </section>
  );
}
