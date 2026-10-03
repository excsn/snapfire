import { Picture } from "@snapfire/fsr-client/react";

import type { AboutProps } from "@generated/client";
import badge from "../../src/img/badge.png";
import ridge from "../../src/img/ridge.jpg";

export default function AboutPage({ photographer }: AboutProps) {
  return (
    <div className="page about">
      <Picture src={photographer.avatar} source="picsum" width={96} height={96} sizes="96px" alt="" className="avatar" />
      <div>
        <h1>
          {photographer.name}
          <img src={badge.src} width={badge.width / 2} height={badge.height / 2} alt="" data-sf-raw className="badge" />
        </h1>
        <p>{photographer.bio}</p>
        <img src={ridge.src} alt="The ridge road, placed as a plain img and served as a picture" className="plain" />
      </div>
    </div>
  );
}
