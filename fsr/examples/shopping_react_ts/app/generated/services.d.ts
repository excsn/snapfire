// Generated from the contract by fsr build. Do not edit.

export interface Product {
  id: bigint;
  name: string;
  brand: string;
  category: string;
  price_cents: bigint;
  list_price_cents?: bigint | null;
  stock: bigint;
  rating: number;
  reviews: bigint;
  description: string;
  tags: string[];
  attributes: Attribute[];
  image: Image;
}

export interface Attribute {
  name: string;
  value: string;
}

export interface Image {
  color: string;
  emoji: string;
}

export interface OrderLine {
  product_id: bigint;
  quantity: bigint;
}

export interface Order {
  id: bigint;
  total_cents: bigint;
  lines: PlacedLine[];
}

export interface PlacedLine {
  product_id: bigint;
  name: string;
  quantity: bigint;
  line_cents: bigint;
}

export interface AddToCart {
  product_id: bigint;
  quantity: bigint;
}

export interface RemoveFromCart {
  product_id: bigint;
}

export interface Session {
  cart: Record<string, bigint>;
}

export interface Services {
  shopping: {
    listProducts(args?: { q?: string | null; category?: string | null; tag?: string | null; }): Promise<Product[]>;
    getProduct(args: { id: bigint; }): Promise<Product>;
    placeOrder(args: { lines: OrderLine[]; }): Promise<Order>;
  };
}
