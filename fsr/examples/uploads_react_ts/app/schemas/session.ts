export interface Held {
  filename: string;
  content_type: string;
  size: bigint;
  caption: string;
}

export interface Session {
  held: Held[];
  error: string;
}

export const defaults: Session = {
  held: [],
  error: "",
};
