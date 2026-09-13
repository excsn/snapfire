import { Mount, useStore } from "@snapfire/fsr-client/react";
import { action, key } from "@snapfire/fsr-client";

/** The symbol the desk is watching, shared with every other island on the page. */
export const watchedKey = key<string>("uni/watched");

interface Quote {
  price: number;
  change: number;
  trail: number[];
}

const buy = action("desk.buy");

export default function Watch({ symbol, quotes, owned }: { symbol: string; quotes: Record<string, Quote>; owned: bigint }) {
  const [held, setHeld] = useStore(watchedKey, symbol);
  const quote = quotes[held];
  return (
    <div className="watch">
      <span className="label">watching</span>
      <strong className="symbol">{held}</strong>
      <span className="price">{quote === undefined ? "\u2014" : quote.price.toFixed(2)}</span>
      {quote === undefined ? null : <Mount module="js/src/ui/Sparkline.vue#default" props={{ points: quote.trail, up: quote.change >= 0 }} />}
      {owned > 0n ? <span className="owned">+{`${owned}`}</span> : null}
      <button type="button" onClick={() => void buy({ symbol: held })}>
        buy 10
      </button>
      <button type="button" onClick={() => setHeld(symbol)}>
        reset
      </button>
    </div>
  );
}
