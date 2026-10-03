import { Picture } from "@snapfire/fsr-client/react";

import type { RootProps } from "@generated/client";
import harbour from "../src/img/harbour.jpg";
import { Wall } from "@src/ui/Wall";

export default function WallPage({ photos }: RootProps) {
  return (
    <div className="page">
      <h1>Four evenings</h1>
      <Picture src={harbour} alt="The harbour wall at dusk, the lead photo" priority sizes="(max-width: 900px) 100vw, 900px" className="lead" />
      <Wall photos={photos} />
    </div>
  );
}
