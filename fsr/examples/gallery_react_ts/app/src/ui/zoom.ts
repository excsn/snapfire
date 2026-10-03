import close from "../img/close.png";

/**
 * A click on a figure opens the photo at the size the browser already chose
 * for it, with a close button. Nothing here renders on the server, so the
 * close icon is an import only the browser's build sees.
 */
export function installZoom(): void {
  document.addEventListener("click", (event) => {
    const target = event.target as Element | null;
    const figure = target?.closest("figure[data-zoom]");
    if (!figure) return;
    const img = figure.querySelector("img");
    if (!img) return;
    event.preventDefault();
    open(img.currentSrc || img.src, img.alt);
  });
}

function open(src: string, alt: string): void {
  const overlay = document.createElement("div");
  overlay.className = "zoom";
  const img = document.createElement("img");
  img.src = src;
  img.alt = alt;
  const button = document.createElement("button");
  button.type = "button";
  button.className = "zoom-close";
  button.setAttribute("aria-label", "Close");
  const icon = document.createElement("img");
  icon.src = close.src;
  icon.width = close.width / 2;
  icon.height = close.height / 2;
  icon.alt = "";
  button.append(icon);
  button.addEventListener("click", () => overlay.remove());
  overlay.addEventListener("click", (event) => {
    if (event.target === overlay) overlay.remove();
  });
  overlay.append(img, button);
  document.body.append(overlay);
}
