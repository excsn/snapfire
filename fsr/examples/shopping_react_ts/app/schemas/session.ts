export interface Session {
  cart: Record<string, bigint>;
}

export const defaults: Session = {
  cart: {},
};
