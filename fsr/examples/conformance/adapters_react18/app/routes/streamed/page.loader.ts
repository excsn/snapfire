export async function load() {
  return { by: "page" };
}

export const store = ({ data }: { data: { by: string } }) => ({ "repro/owner": data.by });
