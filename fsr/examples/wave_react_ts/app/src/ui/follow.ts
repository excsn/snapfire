/** Brings `el` to the middle of what shows it when it is out of view, smoothly unless the reader asks for less motion. What shows it is the part of the transcript inside the window: the transcript scrolls on its own where the panes sit side by side and the window scrolls where they stack. */
export function follow(el: Element): void {
  const box = el.getBoundingClientRect();
  const pane = el.closest(".transcript")?.getBoundingClientRect();
  const top = Math.max(pane?.top ?? 0, 0);
  const bottom = Math.min(pane?.bottom ?? window.innerHeight, window.innerHeight);
  if (box.top >= top && box.bottom <= bottom) return;
  const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  el.scrollIntoView({ behavior: still ? "auto" : "smooth", block: "center" });
}

/** Brings all of `el` into view with the least scrolling, smoothly unless the reader asks for less motion. */
export function reveal(el: Element): void {
  const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  el.scrollIntoView({ behavior: still ? "auto" : "smooth", block: "nearest" });
}

/** Brings the end of the transcript into view, in the transcript where the panes sit side by side and in the window where they stack. `instant` skips the smooth scroll. */
export function toEnd(instant = false): void {
  const last = document.querySelector(".transcript")?.lastElementChild;
  if (!last) return;
  const still = instant || window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  last.scrollIntoView({ behavior: still ? "auto" : "smooth", block: "end" });
}
