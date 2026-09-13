import { action } from "@snapfire/fsr-client";

const lot = action("desk.lot");

/**
 * The lot size a click changes. Written as any React component with a button
 * that calls an action; placed in server mode, so nothing mounts for it: the
 * click posts to the host, Rust runs the handler, dispatches `desk.lot` and
 * renders this again; the page then refreshes the way it does after a
 * browser-mode call.
 */
export default function Lot({ size }: { size: bigint }) {
  return (
    <div className="lot">
      <span className="label">lot</span>
      <button type="button" onClick={() => void lot({ by: -10 })}>
        &minus;
      </button>
      <output>{`${size}`}</output>
      <button type="button" onClick={() => void lot({ by: 10 })}>
        +
      </button>
    </div>
  );
}
