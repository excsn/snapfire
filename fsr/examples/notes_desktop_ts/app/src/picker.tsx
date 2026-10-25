import { useState } from "react";
import { navigate } from "@snapfire/fsr/client";
import { openExternal, openFile } from "@snapfire/fsr/desktop";

export function Picker() {
  const [said, setSaid] = useState("");
  const pick = async () => {
    try {
      const paths = await openFile({ filters: [{ name: "Notes", extensions: ["md", "txt"] }] });
      if (paths) navigate(`/?path=${encodeURIComponent(paths[0])}`);
    } catch (e) {
      setSaid((e as Error).message);
    }
  };
  const docs = async () => {
    try {
      await openExternal("https://github.com/vadimdemedes/ink");
    } catch (e) {
      setSaid((e as Error).message);
    }
  };
  return (
    <div className="picker">
      <button type="button" onClick={pick}>Open a note…</button>
      <button type="button" onClick={docs}>Open a link in the browser</button>
      {said && <p className="said">{said}</p>}
    </div>
  );
}
