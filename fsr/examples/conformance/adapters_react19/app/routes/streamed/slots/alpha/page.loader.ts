export async function load() {
  return { by: "alpha" };
}

export const store = ({ data }: { data: { by: string } }) => ({ "repro/slot": data.by });
