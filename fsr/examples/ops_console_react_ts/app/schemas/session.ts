export interface Session {
  watching: Record<string, boolean>;
  density: string;
}

export const defaults: Session = {
  watching: {},
  density: "comfortable",
};
