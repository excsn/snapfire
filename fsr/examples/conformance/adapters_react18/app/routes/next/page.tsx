import { Place } from "@snapfire/fsr/components";

export default function Next() {
  return (
    <>
      <p className="next-page">The next page, which holds no island.</p>
      <Place name="probe-place" className="next-place" />
    </>
  );
}
