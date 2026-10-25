import { Island } from "@snapfire/fsr/components";
import { Place } from "@snapfire/fsr/components";

import ReactProbe from "@src/ui/ReactProbe";
import VueProbe from "@src/ui/VueProbe.vue";
import ServerProbe from "@src/ui/ServerProbe";
import ServerVueProbe from "@src/ui/ServerVueProbe.vue";
import SvelteProbe from "@src/ui/SvelteProbe.svelte";
import HoldsReact from "@src/ui/HoldsReact";
import HoldsVue from "@src/ui/HoldsVue.vue";
import HoldsSvelte from "@src/ui/HoldsSvelte.svelte";
import Fills from "@src/ui/Fills.vue";

const REACT = "src/ui/ReactProbe.tsx#default";
const VUE = "src/ui/VueProbe.vue#default";

export default function Probes() {
  return (
    <section className="probes">
      <Place name="probe-place" className="home-place" />
      <Island>
        <HoldsReact />
      </Island>
      <Island>
        <HoldsVue />
      </Island>
      <Island>
        <HoldsSvelte />
      </Island>
      <Island>
        <Fills label="filled" />
      </Island>
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
      <Island>
        <SvelteProbe label="svelte" nest={[REACT]}>
          <em>svelte children</em>
        </SvelteProbe>
      </Island>
      <Island mode="server">
        <ServerProbe />
      </Island>
      <Island mode="server">
        <ServerVueProbe />
      </Island>
      <Island define="@src/elements/probe-element.ts">
        <probe-element label="element">
          <em>element children</em>
        </probe-element>
      </Island>
    </section>
  );
}
