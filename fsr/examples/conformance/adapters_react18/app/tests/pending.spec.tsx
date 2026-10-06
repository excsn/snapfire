import * as React from "react";

import { contribute, reset } from "@snapfire/fsr-client/store";
import { describe, expect, render, settle, test } from "@snapfire/fsr-client/testing";

import ReactProbe from "@src/ui/ReactProbe";

describe("a key a streamed segment has yet to seed, under React 18", () => {
  test("suspends a root the server did not render by throwing, since React 18 has no `use`, while a mounted one keeps its value", async () => {
    expect("use" in React, "this React suspends only on a thrown promise").toBeFalsy();
    reset();
    contribute([{ k: "layout", p: [], v: { "probe/count": 1 } }, { k: "page", p: ["content"], v: {}, w: ["probe/count"] }]);
    const view = await render(<ReactProbe label="fresh" nest={[]} />, { hydrate: false });
    expect(view.container.querySelector(".count"), "nothing shows while the key is pending").toBeNull();
    contribute([{ k: "page", p: ["content"], v: { "probe/count": 7 } }]);
    await settle();
    expect(view.container.querySelector(".count")?.textContent).toEqual("7");
    contribute([{ k: "page", p: ["content"], v: {}, w: ["probe/count"] }]);
    await settle();
    expect(view.container.querySelector(".count")?.textContent, "a promise for a mounted root's key leaves what it shows").toEqual("7");
    view.unmount();
  });
});
