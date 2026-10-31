export async function load() {
  return { by: "layout" };
}

export const store = ({ data }: { data: { by: string } }) => ({ "repro/owner": data.by });
