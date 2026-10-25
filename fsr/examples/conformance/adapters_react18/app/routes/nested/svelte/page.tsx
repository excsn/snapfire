import { Island } from "@snapfire/fsr/components";

import Basket from "@src/nested/Basket.svelte";
import Guard from "@src/nested/Guard.svelte";
import Kitchen from "@src/nested/Kitchen.svelte";
import Portions from "@src/nested/Portions.svelte";
import Remains from "@src/nested/Remains.svelte";
import Timer from "@src/nested/Timer.svelte";

export default function KitchenPage() {
  return (
    <section className="page kitchen-page">
      <h2>Cook tonight</h2>
      <Island>
        <Kitchen>
          <Island>
            <Guard>
              <p className="kitchen-note">From the pantry</p>
              <Island>
                <Portions />
              </Island>
              <Island>
                <Basket />
              </Island>
              <Island>
                <Timer />
              </Island>
              <div className="plan-gap" />
              <Island when="visible">
                <Remains />
              </Island>
            </Guard>
          </Island>
        </Kitchen>
      </Island>
    </section>
  );
}
