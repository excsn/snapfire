export interface Session {
  name: string;
  rooms: Record<string, boolean>;
}

export const defaults: Session = {
  name: "",
  rooms: {},
};
