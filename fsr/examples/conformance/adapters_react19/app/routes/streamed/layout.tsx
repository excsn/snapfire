import { Island, Slot, type Children } from "@snapfire/fsr-authoring/template";

import Owner from "@src/ui/Owner";
import OwnerVue from "@src/ui/OwnerVue.vue";
import Bump from "@src/ui/Bump";

export default function StreamedLayout({ children, alpha, beta }: { children: Children; alpha: Children; beta: Children }) {
  return (
    <section className="streamed">
      <Island when="load">
        <Owner name="eager" />
      </Island>
      <Island when="load">
        <OwnerVue name="vue-eager" />
      </Island>
      <Island when="load">
        <Bump />
      </Island>
      {children}
      <Slot name="peek" />
      {alpha}
      {beta}
      <div className="spacer" style={{ height: "3000px" }} />
      <Island when="visible">
        <Owner name="lazy" />
      </Island>
      <Island when="visible">
        <OwnerVue name="vue-lazy" />
      </Island>
    </section>
  );
}
