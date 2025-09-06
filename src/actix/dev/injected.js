// Injected by Snapfire for live-reloading.
(function () {
  const MAX_RETRIES = 10;
  let retryCount = 0;
  let ws;

  function connect() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    // The ws_path will be replaced by the build script or configured by the user.
    // For now, we hardcode the default.
    const wsUrl = `${protocol}//${window.location.host}/_snapfire/ws`;

    ws = new WebSocket(wsUrl);

    ws.onmessage = function (event) {
      if (event.data === 'reload') {
        console.log('[Snapfire] Reloading page...');
        window.location.reload();
      } else if (event.data === 'reload-css') {
        console.log('[Snapfire] Reloading CSS...');
        const links = document.querySelectorAll("link[rel='stylesheet']");
        links.forEach(function (link) {
          const url = new URL(link.href);
          url.searchParams.set('_', Date.now());
          link.href = url.href;
        });
      }
    };

    ws.onopen = function() {
      console.log('[Snapfire] Live-reload connection established.');
      retryCount = 0;
    };

    ws.onclose = function () {
      console.log('[Snapfire] Live-reload connection lost. Retrying...');
      if (retryCount < MAX_RETRIES) {
        retryCount++;
        setTimeout(connect, 1000); // Retry after 1 second
      } else {
        console.error('[Snapfire] Could not reconnect to live-reload server.');
      }
    };
  }

  connect();
})();