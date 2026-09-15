import { get, subscribe } from "@snapfire/fsr-client/store";

import { reservedCount } from "../store.js";

/** The masthead count. The server renders the button and the panel; the element wires the toggle and follows the store. */
class ShedTally extends HTMLElement {
  #stop: (() => void) | null = null;

  connectedCallback(): void {
    const button = this.querySelector<HTMLButtonElement>("button.tally");
    const panel = this.querySelector<HTMLElement>(".tally-panel");
    if (!button || !panel) return;
    const toggle = () => {
      panel.hidden = !panel.hidden;
      button.setAttribute("aria-expanded", String(!panel.hidden));
    };
    const show = (count: unknown) => {
      if (typeof count === "number") button.textContent = `${count} reserved`;
    };
    button.addEventListener("click", toggle);
    show(get(reservedCount));
    const unsubscribe = subscribe(reservedCount, show);
    this.#stop = () => {
      button.removeEventListener("click", toggle);
      unsubscribe();
    };
  }

  disconnectedCallback(): void {
    this.#stop?.();
    this.#stop = null;
  }
}

customElements.define("shed-tally", ShedTally);
