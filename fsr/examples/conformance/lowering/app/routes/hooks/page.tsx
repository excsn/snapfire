import { Island } from "@snapfire/fsr/components";

import Counters from "@src/ui/Counters";
import Fields from "@src/ui/Fields";
import Themed from "@src/ui/Themed";
import Trend from "@src/ui/Trend";

export default function Hooks() {
  return (
    <section className="hooks">
      <Island>
        <Counters />
      </Island>
      <Island>
        <Themed mode="dark" />
      </Island>
      <Island>
        <Fields />
      </Island>
      <Island>
        <Trend start={3} />
      </Island>
    </section>
  );
}
