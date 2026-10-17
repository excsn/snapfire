import { useState } from "react";

export default function Tabs({ ids, getHeader, renderContent }: { ids: string[]; getHeader: (id: string) => string; renderContent: (id: string) => React.ReactNode }) {
  const [selected, setSelected] = useState(ids[0]);
  return (
    <div className="tabs">
      {ids.map((id) => (
        <button key={id} onClick={() => setSelected(id)}>
          {getHeader(id)}
        </button>
      ))}
      <div className="tab-content">{renderContent(selected)}</div>
    </div>
  );
}
