import { Island } from "@snapfire/fsr-authoring/template";

import Shelf from "@src/vue/Shelf.vue";

const ITEMS = [
  { name: "pear", stock: 3 },
  { name: "fig", stock: 1 },
];

export default function VuePage() {
  return (
    <section className="vue">
      <Island>
        <Shelf items={ITEMS} />
      </Island>
    </section>
  );
}
