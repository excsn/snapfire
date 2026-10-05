import { Island } from "@snapfire/fsr-authoring/template";

import Picker from "@src/ui/Picker";

interface Data {
  cheapest: string[];
  tags: string[];
  priciest: number;
  last: string;
  codes: string[];
  json: string;
  squared: number;
}

export default function Methods({ cheapest, tags, priciest, last, codes, json, squared }: Data) {
  return (
    <section className="methods">
      <p className="cheapest">{cheapest.join(", ")}</p>
      <p className="tags">{tags.slice().sort().join(" ")}</p>
      <p className="priciest">{priciest.toFixed(2)}</p>
      <p className="last">{last}</p>
      <p className="codes">{codes.concat(["END"]).join("|")}</p>
      <p className="json">{json}</p>
      <p className="squared">{squared}</p>
      <p className="reversed">{[...tags].reverse().join(" ")}</p>
      <Island>
        <Picker names={cheapest} />
      </Island>
    </section>
  );
}
