/** When a talk starts, in words. The server writes the time; the element writes the sentence once it is defined and again whenever the server changes it. */
class TalkTime extends HTMLElement {
  static observedAttributes: string[] = ["starts"];

  connectedCallback(): void {
    this.write();
  }

  attributeChangedCallback(): void {
    if (this.isConnected) this.write();
  }

  private write(): void {
    this.textContent = `at ${this.getAttribute("starts") ?? ""}`;
  }
}

customElements.define("talk-time", TalkTime);

export default TalkTime;
