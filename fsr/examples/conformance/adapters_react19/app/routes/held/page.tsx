import { Island } from "@snapfire/fsr/components";

import HeldProbe from "@src/ui/HeldProbe";
import HeldVueProbe from "@src/ui/HeldVueProbe.vue";
import LazyProbe from "@src/ui/LazyProbe";
import LazyVueProbe from "@src/ui/LazyVueProbe.vue";

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
      <Island>
        <LazyProbe label="react-lazy" />
      </Island>
      <Island>
        <LazyVueProbe label="vue-lazy" />
      </Island>
    </section>
  );
}
