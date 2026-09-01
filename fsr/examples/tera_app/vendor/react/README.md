# Vendored React runtime

The example is an application, so it serves its vendor tree itself. Place browser-native ES module builds of React here before opening the pages in a browser:

- `react.js`, the `react` package as one ESM file exporting its named API
- `react-dom-client.js`, `react-dom/client` as one ESM file exporting `createRoot` and `hydrateRoot`, with its `react` import pointing at `./react.js`

Without these files the pages still render and stream; islands log a mount warning and stay server-rendered.
