import { useEffect } from "react";
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
const watch = action("desk.watch");

export default function Watch({ symbol, quotes, owned, lot }: { symbol: string; quotes: Record<string, Quote>; owned: bigint; lot: bigint }) {
  const [held, setHeld] = useStore(watchedKey, symbol);
  const symbols = Object.keys(quotes);
  const at = symbols.indexOf(held);
  const turn = (by: number) => setHeld(symbols[(at + by + symbols.length) % symbols.length] ?? symbol);
  // The store is the page's; the session is the server's. Whoever moved the
  // store, this island tells the server; the refresh hands the symbol back
  // as `symbol`, which is what a reload renders from.
  useEffect(() => {
    if (held !== symbol) void watch({ symbol: held });
  }, [held, symbol]);
  const quote = quotes[held];
  return (
    <div className="watch">
      <span className="label">watching</span>
      <button type="button" className="turn" aria-label="previous" onClick={() => turn(-1)}>
        &lsaquo;
      </button>
      <strong className="symbol">{held}</strong>
      <button type="button" className="turn" aria-label="next" onClick={() => turn(1)}>
        &rsaquo;
      </button>
      <span className="price">{quote === undefined ? "\u2014" : quote.price.toFixed(2)}</span>
      {quote === undefined ? null : <Mount module="js/src/ui/Sparkline.vue#default" props={{ points: quote.trail, up: quote.change >= 0 }} />}
      {owned > 0n ? <span className="owned">+{`${owned}`}</span> : null}
      <button type="button" onClick={() => void buy({ symbol: held })}>
        buy {`${lot}`}
      </button>
    </div>
  );
}
