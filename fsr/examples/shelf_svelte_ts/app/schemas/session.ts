export interface Session {
  reading: string;
  progress: Record<string, number>;
  done: Record<string, boolean>;
}

export const defaults: Session = {
  reading: "",
  progress: {},
  done: {},
};
