use snapfire_fsr::Plan;

/// The build artifact `fsr build app` writes: routes, lowered loaders and
/// lowered actions. Nothing here describes a route or a body; the file does.
pub const PLAN: &str = include_str!("../../app/plan.json");

/// The contract the same build writes: the imported shopping service plus the
/// session and input types declared under `app/schemas/`.
pub const CONTRACT: &str = include_str!("../../app/generated/contract.json");

/// A route the file system convention does not describe, added in Rust beside
/// the ones the plan file carries.
pub fn about_plan() -> Plan {
  Plan::of("shell#document").slot("content", Plan::of("src/About.tsx#default"))
}
