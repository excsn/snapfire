import { Link } from "@snapfire/fsr-client/react";

import { actions, type ProductProps } from "@generated/client";
import { addedToCart, failed } from "@src/ui/feedback";
import { money } from "@src/ui/money";
import { stockLine } from "@src/ui/ProductCard";
import { Stars } from "@src/ui/Stars";
import { Thumb } from "@src/ui/Thumb";

export default function QuickLook({ product, stock: level, inCart }: ProductProps) {
  const line = stockLine(product.stock);
  const held = Number(inCart);

  async function add(): Promise<void> {
    try {
      const result = await actions.cart.addToCart({ product_id: product.id, quantity: 1n });
      addedToCart(product.name, result.count);
    } catch (e) {
      failed(e);
    }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={product.name}>
      <div className="modal quick-look">
        <button className="modal-close" aria-label="Close" onClick={() => history.back()}>
          ×
        </button>
        <Thumb image={product.image} />
        <div className="quick-look-body">
          <h2>{product.name}</h2>
          <p className="product-brand">by {product.brand}</p>
          <Stars rating={product.rating} reviews={product.reviews} />
          <p className="price-line price-line-big">
            <span className="price">{money(product.price_cents)}</span>
          </p>
          <p className={line.className}>{line.text}</p>
          <p className="product-description">{product.description}</p>
          <p className="buy-note">{Number(level.on_hand)} on hand in the {level.warehouse} warehouse.</p>
          {held > 0 ? <p className="in-cart">{held} in your cart.</p> : null}
          <div className="quick-look-actions">
            <button className="btn btn-primary" disabled={Number(product.stock) === 0} onClick={() => void add()}>
              Add to cart
            </button>
            <Link className="btn btn-secondary" href={`/product/${String(product.id)}`} full>
              Full details
            </Link>
          </div>
        </div>
      </div>
    </div>
  );
}
