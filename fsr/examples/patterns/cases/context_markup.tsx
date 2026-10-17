import { createContext, useContext } from "react";
const Slot = createContext<any>(null);
function Shows() {
  const node = useContext(Slot);
  return <aside>{node}</aside>;
}
export default function Page() {
  return <Slot.Provider value={<b>from context</b>}><Shows /></Slot.Provider>;
}
