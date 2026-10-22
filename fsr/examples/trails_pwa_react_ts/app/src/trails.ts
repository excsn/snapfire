export interface Trail {
  id: string;
  name: string;
  region: string;
  km: number;
  climb: number;
  notes: string[];
}

export const trails: Trail[] = [
  { id: "ridge", name: "Long Ridge", region: "North Downs", km: 14.2, climb: 410, notes: ["The stile after the second gate is broken; use the gap to its left.", "No water between the car park and the beacon."] },
  { id: "marsh", name: "Saltmarsh Loop", region: "Estuary", km: 8.5, climb: 40, notes: ["The causeway floods two hours either side of high tide.", "Birds on the left bank in winter; keep dogs on a lead."] },
  { id: "beech", name: "Beech Hangers", region: "Weald", km: 11.0, climb: 280, notes: ["Steep and slippery after rain on the descent through the hanger.", "The tea room by the church closes at four."] },
  { id: "chalk", name: "Chalk Figure Walk", region: "Vale", km: 6.3, climb: 190, notes: ["The figure is best seen from the lane below, not from on top of it.", "Signal drops out in the coombe."] },
];
