// Injected by SnapFire for live-reloading.
(function () {
  const MAX_RETRIES = 10;
  let retryCount = 0;
  let ws;

  function connect() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    // __SNAPFIRE_WS_PATH__ is substituted by the injecting middleware.
    const wsUrl = `${protocol}//${window.location.host}__SNAPFIRE_WS_PATH__`;

    ws = new WebSocket(wsUrl);

    ws.onmessage = function (event) {
      if (event.data === 'reload') {
        console.log('[SnapFire] Reloading page...');
        window.location.reload();
      } else if (event.data === 'reload-css') {
        console.log('[SnapFire] Reloading CSS...');
        const links = document.querySelectorAll("link[rel='stylesheet']");
        links.forEach(function (link) {
          const url = new URL(link.href);
          url.searchParams.set('_', Date.now());
          link.href = url.href;
        });
      }
    };

    ws.onopen = function() {
      console.log('[SnapFire] Live-reload connection established.');
      retryCount = 0;
    };

    ws.onclose = function () {
      console.log('[SnapFire] Live-reload connection lost. Retrying...');
      if (retryCount < MAX_RETRIES) {
        retryCount++;
        setTimeout(connect, 1000); // Retry after 1 second
      } else {
        console.error('[SnapFire] Could not reconnect to live-reload server.');
      }
    };
  }

  connect();
})();