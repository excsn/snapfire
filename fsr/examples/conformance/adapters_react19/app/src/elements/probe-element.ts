/** The custom element probe: the server writes its label and children, the element marks itself once it is defined and counts its disconnections. */
import { unmounted } from "@src/probes";

class ProbeElement extends HTMLElement {
  connectedCallback(): void {
    this.setAttribute("data-upgraded", "");
  }

  disconnectedCallback(): void {
    unmounted("element");
  }
}

customElements.define("probe-element", ProbeElement);

export default ProbeElement;
