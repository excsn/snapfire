import { Island, Link } from "@snapfire/fsr-authoring/template";

import Owner from "@src/ui/Owner";
import OwnerVue from "@src/ui/OwnerVue.vue";

export default function Streamed({ by }: { by: string }) {
  return (
    <>
      <p className="page-by">{by}</p>
      <Island when="load">
        <Owner name="page-react" />
      </Island>
      <Island when="load">
        <OwnerVue name="page-vue" />
      </Island>
      <Link href="/streamed/other" className="to-other">
        other
      </Link>
    </>
  );
}
