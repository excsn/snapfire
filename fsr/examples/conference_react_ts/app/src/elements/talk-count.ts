/** The number of talks in words. The server writes the number; the element writes the sentence once it is defined. */
class TalkCount extends HTMLElement {
  connectedCallback(): void {
    const talks = Number(this.getAttribute("talks") ?? "0");
    this.textContent = talks === 1 ? "One talk today" : `${talks} talks today`;
    this.setAttribute("data-upgraded", "");
  }
}

customElements.define("talk-count", TalkCount);

export default TalkCount;
