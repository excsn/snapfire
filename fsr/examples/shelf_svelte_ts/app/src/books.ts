export interface Book {
  id: string;
  title: string;
  author: string;
  pages: number;
}

export const books: Book[] = [
  { id: "earthsea", title: "A Wizard of Earthsea", author: "Ursula K. Le Guin", pages: 183 },
  { id: "piranesi", title: "Piranesi", author: "Susanna Clarke", pages: 272 },
  { id: "remains", title: "The Remains of the Day", author: "Kazuo Ishiguro", pages: 245 },
  { id: "stoner", title: "Stoner", author: "John Williams", pages: 278 },
  { id: "dispossessed", title: "The Dispossessed", author: "Ursula K. Le Guin", pages: 387 },
  { id: "gilead", title: "Gilead", author: "Marilynne Robinson", pages: 247 },
];
