import { useStore } from "@snapfire/fsr-client/react";
import { key } from "@snapfire/fsr-client";

/** The symbol the desk is watching, shared with every other island on the page. */
export const watchedKey = key<string>("uni/watched");

export default function Watch({ symbol, prices }: { symbol: string; prices: Record<string, number> }) {
  const [held, setHeld] = useStore(watchedKey, symbol);
  const price = prices[held];
  return (
    <div className="watch">
      <span className="label">watching</span>
      <strong className="symbol">{held}</strong>
      <span className="price">{price === undefined ? "\u2014" : price.toFixed(2)}</span>
      <button type="button" onClick={() => setHeld(symbol)}>
        reset
      </button>
    </div>
  );
}
