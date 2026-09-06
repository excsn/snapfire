export interface Session {
  name: string;
  waves: Record<string, boolean>;
}

export const defaults: Session = {
  name: "",
  waves: {},
};
