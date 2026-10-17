import { navigate } from "@snapfire/fsr-client";
import { ctx, expect, load, screen, test } from "@snapfire/fsr-client/testing";

const filament = { id: 1n, name: "PLA filament", brand: "Prusa", category: "printing", price_cents: 2400n, list_price_cents: 2900n, image: { file: "1.png", color: "#e8d5b5", emoji: "🧵" }, rating: 4.5, reviews: 12n, stock: 5n, description: "A spool.", tags: ["pla"], attributes: [] };

test("a stylesheet the next page does not name stays until the page that used it is gone", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  const leaving = document.createElement("link");
  leaving.rel = "stylesheet";
  leaving.setAttribute("data-sf-css", "");
  leaving.href = "/static/css/leaving.css";
  document.head.appendChild(leaving);
  let catalogShownAtRemoval: boolean | null = null;
  leaving.remove = function () {
    catalogShownAtRemoval = screen.queryByText("Today's picks") !== null;
    HTMLLinkElement.prototype.remove.call(this);
  };
  await navigate("/cart");
  expect(catalogShownAtRemoval, "the catalog's stylesheet went only once the catalog had left the page").toBe(false);
  expect(leaving.isConnected).toBe(false);
});
