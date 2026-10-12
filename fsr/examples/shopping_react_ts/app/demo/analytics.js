// A stand-in for an analytics vendor's tag, served from this app so the example needs no network.
(function () {
  if (window.demoAnalytics) return;
  var views = [location.pathname];
  window.demoAnalytics = { loads: 1, views: views };
  document.addEventListener("sf:navigate", function (event) {
    views.push(event.detail.path);
  });
})();
