export interface Session {
  planned: Record<string, boolean>;
}

export const defaults: Session = {
  planned: {},
};
