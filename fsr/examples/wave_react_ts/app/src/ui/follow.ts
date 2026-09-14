/** Brings `el` to the middle of what shows it when it is out of view, smoothly unless the reader asks for less motion. What shows it is the part of the transcript inside the window: the transcript scrolls on its own where the panes sit side by side and the window scrolls where they stack. */
export function follow(el: Element): void {
  const box = el.getBoundingClientRect();
  const pane = el.closest(".blips")?.getBoundingClientRect();
  const top = Math.max(pane?.top ?? 0, 0);
  const bottom = Math.min(pane?.bottom ?? window.innerHeight, window.innerHeight);
  if (box.top >= top && box.bottom <= bottom) return;
  const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  el.scrollIntoView({ behavior: still ? "auto" : "smooth", block: "center" });
}
