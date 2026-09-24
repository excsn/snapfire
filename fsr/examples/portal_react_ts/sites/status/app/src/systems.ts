export interface System {
  name: string;
  owner: string;
  state: "operational" | "degraded" | "down";
  note: string;
}

export const systems: System[] = [
  { name: "Portal", owner: "Platform", state: "operational", note: "Sign-in and the directory." },
  { name: "Billing", owner: "Finance", state: "operational", note: "Invoices and payments." },
  { name: "Blog", owner: "Marketing", state: "degraded", note: "New posts publish on the next deploy." },
  { name: "Ledger", owner: "Finance", state: "operational", note: "The service billing reads invoices from." },
];
