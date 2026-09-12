export interface Session {
  /** Tool id to the loan length agreed when it was reserved, in days. */
  reserved: Record<string, number>;
}

export const defaults: Session = {
  reserved: {},
};
