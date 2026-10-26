const notices = ["The cloakroom closes at six.", "Hall B's lift is out of order; the stairs are by the café."];

export async function load() {
  return { notices };
}

export const store = ({ data }: { data: { notices: string[] } }) => ({ "venue/notices": data.notices.length });
