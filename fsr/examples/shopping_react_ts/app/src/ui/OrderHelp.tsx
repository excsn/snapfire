import { useState } from "react";

export function OrderHelp({ orderId }: { orderId: bigint | number }) {
  const [open, setOpen] = useState(false);
  const [asked, setAsked] = useState(0);
  return (
    <section className="order-help">
      <h2>Need help with this order?</h2>
      <p>Quote order #{String(orderId)} when you write to us.</p>
      <button
        className="btn"
        onClick={() => {
          if (!open) setAsked(asked + 1);
          setOpen(!open);
        }}
      >
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
      {asked > 1 ? <p className="asked-often">Opened {asked} times. Chat is the fastest way to reach us.</p> : null}
    </section>
  );
}
