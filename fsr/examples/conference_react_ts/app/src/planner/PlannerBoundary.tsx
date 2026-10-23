import { Component, Suspense, type ReactNode } from "react";

export default class PlannerBoundary extends Component<{ children?: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  render(): ReactNode {
    if (this.state.failed) {
      return (
        <div className="planner-failed">
          The planner hit a problem.
          <button className="planner-retry" onClick={() => this.setState({ failed: false })}>Try again</button>
        </div>
      );
    }
    return <Suspense fallback={<p className="planner-loading">Loading the planner</p>}>{this.props.children}</Suspense>;
  }
}
