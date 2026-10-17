import { Island } from "@snapfire/fsr-authoring/template";

import Counters from "@src/ui/Counters";
import Fields from "@src/ui/Fields";
import Themed from "@src/ui/Themed";

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
    </section>
  );
}
