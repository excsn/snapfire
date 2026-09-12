/** A due date as a distance from today. The server writes the date; the element rewrites it once it knows what day it is. */
class TimeAgo extends HTMLElement {
  connectedCallback(): void {
    const when = this.getAttribute("datetime");
    if (!when) return;
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const days = Math.round((new Date(when).getTime() - today.getTime()) / 86_400_000);
    this.textContent = days < 0 ? `${-days} ${days === -1 ? "day" : "days"} overdue` : days === 0 ? "back today" : `back in ${days} ${days === 1 ? "day" : "days"}`;
    this.classList.toggle("overdue", days < 0);
  }
}

customElements.define("time-ago", TimeAgo);
