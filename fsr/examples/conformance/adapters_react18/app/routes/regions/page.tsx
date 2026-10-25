import { Island } from "@snapfire/fsr/components";

import Frame from "@src/ui/Frame";
import FrameVue from "@src/ui/FrameVue.vue";
import FrameSvelte from "@src/ui/FrameSvelte.svelte";

export default function Regions() {
  return (
    <section className="regions">
      <Island>
        <Frame>
          <p className="bound" data-in="react">
            server
          </p>
        </Frame>
      </Island>
      <Island>
        <FrameVue>
          <p className="bound" data-in="vue">
            server
          </p>
        </FrameVue>
      </Island>
      <Island>
        <FrameSvelte>
          <p className="bound" data-in="svelte">
            server
          </p>
        </FrameSvelte>
      </Island>
    </section>
  );
}
