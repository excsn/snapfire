/** A loan length picker inside a declarative shadow root the server wrote. The parser attaches the root on a full page; after a swap the element attaches it itself. */
class LoanPlanner extends HTMLElement {
  connectedCallback(): void {
    const root = this.shadowRoot ?? this.#attach();
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
    };
    range.addEventListener("input", show);
    show();
  }

  #attach(): ShadowRoot | null {
    const template = this.querySelector<HTMLTemplateElement>("template[shadowrootmode]");
    if (!template) return null;
    const root = this.attachShadow({ mode: "open" });
    root.append(template.content.cloneNode(true));
    template.remove();
    return root;
  }
}

customElements.define("loan-planner", LoanPlanner);
