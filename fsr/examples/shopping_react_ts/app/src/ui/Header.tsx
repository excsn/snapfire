import { useState } from "react";
import { navigate } from "@snapfire/fsr-client";
import { useStore } from "@snapfire/fsr-client/react";

import { cartCount } from "@src/store";

import { categories } from "./categories";

export function Header({ q = "", category = "" }: { q?: string | null; category?: string | null }) {
  const [text, setText] = useState(q ?? "");
  const [chosen, setChosen] = useState(category ?? "");
  const [items] = useStore(cartCount, 0);

  function search(): void {
    const params = new URLSearchParams();
    if (text.trim()) params.set("q", text.trim());
    if (chosen) params.set("category", chosen);
    const query = params.toString();
    void navigate(query ? `/?${query}` : "/");
  }

  return (
    <header className="site-header">
      <a className="brand" href="/">
        <span className="brand-mark">sf</span>
        <span className="brand-name">snapfire.shop</span>
      </a>
      <form
        className="search"
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          search();
        }}
      >
        <select aria-label="Category" value={chosen} onChange={(e) => setChosen(e.target.value)}>
          <option value="">All</option>
          {categories.map((c) => (
            <option key={c.key} value={c.key}>
              {c.label}
            </option>
          ))}
        </select>
        <input
          name="q"
          type="search"
          placeholder="Search snapfire.shop"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <button type="submit" aria-label="Search">
          🔍
        </button>
      </form>
      <nav className="site-nav">
        <a href="/about">About</a>
        <a className="cart-link" href="/cart" aria-label={`Cart, ${items} items`}>
          <span className="cart-icon">🛒</span>
          <span className={items > 0 ? "badge" : "badge badge-empty"}>{items}</span>
          <span className="cart-word">Cart</span>
        </a>
      </nav>
    </header>
  );
}
