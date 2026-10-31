export async function load() {
  return { by: "beta" };
}

const seed = (by: string) => ({ "repro/slot": by });

export const store = ({ data }: { data: { by: string } }) => ({ ...seed(data.by) });
