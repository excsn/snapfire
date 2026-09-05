import { useState } from "react";

export function OrderHelp({ orderId }: { orderId: bigint | number }) {
  const [open, setOpen] = useState(false);
  return (
    <section className="order-help">
      <h2>Need help with this order?</h2>
      <p>Quote order #{String(orderId)} when you write to us.</p>
      <button className="btn" onClick={() => setOpen(!open)}>
        {open ? "Hide contact options" : "Show contact options"}
      </button>
      {open ? (
        <ul className="contact-options">
          <li>
            <a href="mailto:help@snapfire.shop">help@snapfire.shop</a>
          </li>
          <li>Chat, weekdays 9 to 5</li>
        </ul>
      ) : null}
    </section>
  );
}
