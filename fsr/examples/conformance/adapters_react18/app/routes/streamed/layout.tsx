import { Island, type Children } from "@snapfire/fsr-authoring/template";

import OwnerForeign from "@src/ui/OwnerForeign.vue";
import OwnerBrowser from "@src/ui/OwnerBrowser";

export default function StreamedLayout({ children }: { children: Children }) {
  return (
    <section className="streamed">
      <Island when="load">
        <OwnerForeign name="vue-foreign" />
      </Island>
      <Island when="load">
        <OwnerBrowser name="react-browser" />
      </Island>
      {children}
    </section>
  );
}
