use snapfire_fsr::Plan;

/// The build artifact. Nothing here describes a route; the file does.
pub const PLAN: &str = include_str!("../../app/plan.json");

/// A route the file system convention does not describe, added in Rust beside
/// the ones the plan file carries.
pub fn about_plan() -> Plan {
  Plan::of("shell#document").slot("content", Plan::of("app/main.tsx#About"))
}
