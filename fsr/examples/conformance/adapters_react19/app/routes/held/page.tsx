import { Island } from "@snapfire/fsr-authoring/template";

import HeldProbe from "@src/ui/HeldProbe";
import HeldVueProbe from "@src/ui/HeldVueProbe.vue";

export default function Held() {
  return (
    <section className="held-probes">
      <Island>
        <HeldProbe label="react" />
      </Island>
      <div className="spacer" style={{ height: "3000px" }} />
      <Island when="visible">
        <HeldVueProbe label="vue" />
      </Island>
    </section>
  );
}
