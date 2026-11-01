/** The number of talks in words. The server writes the number and the track; the element writes the sentence once it is defined and again whenever the server changes them. */
class TalkCount extends HTMLElement {
  static observedAttributes: string[] = ["talks", "track"];

  connectedCallback(): void {
    this.write();
    this.setAttribute("data-upgraded", "");
  }

  attributeChangedCallback(): void {
    if (this.isConnected) this.write();
  }

  private write(): void {
    const talks = Number(this.getAttribute("talks") ?? "0");
    const track = this.getAttribute("track");
    const kind = track === null ? "" : `${track} `;
    this.textContent = talks === 1 ? `One ${kind}talk today` : `${talks} ${kind}talks today`;
  }
}

customElements.define("talk-count", TalkCount);

export default TalkCount;
