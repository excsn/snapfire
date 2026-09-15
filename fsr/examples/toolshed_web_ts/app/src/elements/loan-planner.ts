import { shadowOf } from "@snapfire/fsr-client/elements";

/** A loan length picker inside the shadow root the server writes from `elements/loan-planner.tsx`. Form-associated, so the length inside the shadow root is posted with the reservation. */
class LoanPlanner extends HTMLElement {
  static formAssociated = true;
  #internals = this.attachInternals();
  #stop: (() => void) | null = null;

  connectedCallback(): void {
    const root = shadowOf(this, this.#internals);
    if (!root) return;
    const range = root.querySelector<HTMLInputElement>("input[name=days]");
    const out = root.querySelector("output");
    const back = root.querySelector("[data-back]");
    if (!range || !out || !back) return;
    const show = () => {
      const days = Number(range.value);
      out.textContent = `${days} ${days === 1 ? "day" : "days"}`;
      const date = new Date(Date.now() + days * 86_400_000);
      back.textContent = `by ${date.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}`;
      this.#internals.setFormValue(range.value);
    };
    range.addEventListener("input", show);
    show();
    this.#stop = () => range.removeEventListener("input", show);
  }

  disconnectedCallback(): void {
    this.#stop?.();
    this.#stop = null;
  }
}

customElements.define("loan-planner", LoanPlanner);
