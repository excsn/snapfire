export type Photo = { file: string; title: string; place: string; taken: string };

export const photos: Photo[] = [
  { file: "harbour.jpg", title: "Harbour wall", place: "Dún Laoghaire", taken: "2026-08-02" },
  { file: "ridge.jpg", title: "The ridge road", place: "Glendalough", taken: "2026-08-09" },
  { file: "dunes.jpg", title: "Dunes after rain", place: "Brittas Bay", taken: "2026-08-16" },
  { file: "marsh.jpg", title: "Marsh light", place: "Bull Island", taken: "2026-08-23" },
];

/** The photographer's avatar is an id on a remote image service, not a file here. */
export const photographer = { name: "Ada Quill", avatar: "1027", bio: "Four evenings, one phone, no edits." };
