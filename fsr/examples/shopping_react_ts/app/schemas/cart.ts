export interface AddToCart {
  product_id: bigint;
  quantity: bigint;
}

export interface RemoveFromCart {
  product_id: bigint;
}
