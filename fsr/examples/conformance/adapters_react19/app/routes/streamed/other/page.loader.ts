export async function load() {
  return { by: "other" };
}

export const store = ({ data }: { data: { by: string } }) => ({ "repro/owner": data.by });
