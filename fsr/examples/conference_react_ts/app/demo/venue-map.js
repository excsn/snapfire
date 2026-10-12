// A stand-in for a map vendor's widget, served from this app so the example needs no network.
(function () {
  if (window.venueMap) return;
  function draw() {
    document.querySelectorAll("[data-venue-map]").forEach(function (el) {
      el.textContent = "Map: " + el.getAttribute("data-venue-map");
      el.setAttribute("data-drawn", "");
    });
  }
  window.venueMap = { loads: 1, draw: draw };
  draw();
  document.addEventListener("sf:navigate", draw);
})();
