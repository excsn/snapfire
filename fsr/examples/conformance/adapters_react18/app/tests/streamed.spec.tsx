import { describe, expect, load, settle, test } from "@snapfire/fsr-client/testing";

const shown = () => [...document.querySelectorAll(".owner-unrendered")].map((el) => `${(el as HTMLElement).dataset.island}=${el.textContent}`);

describe("an island the server cannot render, under a page that streams a key it reads", () => {
  test("is held by the keys the build read from its source", async () => {
    const html = await (await fetch("/streamed")).text();
    const wave = html.slice(0, html.indexOf("<template data-sf-fill"));
    for (const module of ["src/ui/OwnerForeign.vue#default", "src/ui/OwnerBrowser.tsx#default"]) {
      const marker = wave.indexOf(`data-sf-module="${module}"`);
      expect(marker >= 0, `${module} is placed`).toBeTruthy();
      const props = wave.slice(marker, wave.indexOf("</script>", marker));
      expect(props.includes('"$aw":["repro/owner"]'), `${module} waits for the key it reads: ${props}`).toBeTruthy();
    }
  });

  test("shows the page's value once it mounts", async () => {
    await load("/streamed");
    await settle();
    expect(shown()).toEqual(["vue-foreign=page", "react-browser=page"]);
  });
});
