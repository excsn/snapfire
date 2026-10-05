import { Island } from "@snapfire/fsr-authoring/template";

import ReactProbe from "@src/ui/ReactProbe";
import VueProbe from "@src/ui/VueProbe.vue";
import ServerProbe from "@src/ui/ServerProbe";

const REACT = "src/ui/ReactProbe.tsx#default";
const VUE = "src/ui/VueProbe.vue#default";

export default function Probes() {
  return (
    <section className="probes">
      <Island>
        <ReactProbe label="react" nest={[VUE]}>
          <em>react children</em>
        </ReactProbe>
      </Island>
      <Island>
        <VueProbe label="vue" nest={[REACT]}>
          <em>vue children</em>
        </VueProbe>
      </Island>
      <Island mode="server">
        <ServerProbe />
      </Island>
      <Island define="@src/elements/probe-element.ts">
        <probe-element label="element">
          <em>element children</em>
        </probe-element>
      </Island>
    </section>
  );
}
