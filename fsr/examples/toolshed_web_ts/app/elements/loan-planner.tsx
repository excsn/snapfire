export default function LoanPlanner({ deposit, days, max, disabled }: { deposit: number | bigint; days: number; max: number | bigint; disabled?: boolean }) {
  return (
    <>
      <style>{":host { display: block; margin: 0 0 16px; padding: 14px 16px; border: 1px solid #e3e5ea; border-radius: 8px; background: #fff; } :host([disabled]) { background: #f7f8fa; } label { display: flex; align-items: center; gap: 10px; } input { flex: 1; } input:disabled { cursor: default; } output { min-width: 5em; font-weight: 600; } p { margin: 8px 0 0; color: #6b7280; font-size: 13px; }"}</style>
      <label>
        {disabled ? "Borrowed for" : "Borrow for"}
        <input type="range" name="days" min="1" max={`${max}`} value={`${days}`} disabled={disabled} />
        <output>{days} days</output>
      </label>
      <p>
        Back <b data-back>in {days} days</b>, £{deposit} {disabled ? "is held" : "would be held"} until then.
      </p>
    </>
  );
}
