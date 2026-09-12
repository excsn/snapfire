export interface Session {
  reserved: Record<string, boolean>;
}

export const defaults: Session = {
  reserved: {},
};
