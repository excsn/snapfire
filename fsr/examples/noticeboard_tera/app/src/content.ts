export type Notice = { id: string; title: string; body: string; posted: string };

export const notices: Notice[] = [
  { id: "roof", title: "Roof work on the east stair", body: "The east stair is closed from Monday to Wednesday while the flashing is replaced. Use the west stair.", posted: "2026-09-15" },
  { id: "bins", title: "Bins go out on Thursday this week", body: "The collection moved a day because of the holiday. Paper and glass together this time.", posted: "2026-09-16" },
  { id: "garden", title: "Garden afternoon", body: "Saturday from two. Bring gloves; the shed has the rest.", posted: "2026-09-17" },
];
