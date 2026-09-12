export interface ReserveTool {
  tool_id: string;
  /** Omitted by a form the browser never ran the planner in; the tool's own limit stands. */
  days?: number;
}

export interface ReleaseTool {
  tool_id: string;
}
