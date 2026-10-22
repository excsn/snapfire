import { load } from "../../routes/slots/alerts/page.loader";
import { ctx, expect, fn, test } from "@snapfire/fsr/testing";

test("the alerts slot lists what the fleet answers", async () => {
  const listAlerts = fn();
  listAlerts.mockResolvedValue([{ id: 21n, agent_id: 3n, level: "page", text: "builder-us-1 stopped answering" }]);
  const c = ctx({ services: { fleet: { listAlerts } } });
  const result = await load(c);
  expect(result.alerts.length).toEqual(1);
  expect(result.down).toEqual("");
});

test("the alerts slot says why when the fleet does not answer", async () => {
  const listAlerts = fn();
  listAlerts.mockRejectedValue({ kind: "unavailable", message: "the fleet is restarting" });
  const c = ctx({ services: { fleet: { listAlerts } } });
  const result = await load(c);
  expect(result.alerts).toEqual([]);
  expect(result.down).toEqual("the fleet is restarting");
});
