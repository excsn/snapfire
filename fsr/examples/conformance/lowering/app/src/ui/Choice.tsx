export default function Choice({ label, chosen, choose }: { label: string; chosen: boolean; choose: () => void }) {
  return (
    <button className={chosen ? "choice chosen" : "choice"} onClick={choose}>
      {label}
    </button>
  );
}
