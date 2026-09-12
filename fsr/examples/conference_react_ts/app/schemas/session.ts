export interface Session {
  saved: Record<string, boolean>;
}

export const defaults: Session = {
  saved: {},
};
