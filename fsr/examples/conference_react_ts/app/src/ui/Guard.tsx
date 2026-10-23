import { Component, type ReactNode } from "react";

/** Keeps the page standing when something inside throws while rendering in the browser. */
export default class Guard extends Component<{ children?: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  render(): ReactNode {
    if (this.state.failed) return <p className="quiet">This part of the schedule failed to render.</p>;
    return this.props.children;
  }
}
