import { Component, type ErrorInfo, type ReactNode } from "react";
import { Banner } from "./Banner";

export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  override state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(error, info.componentStack);
  }

  override render() {
    if (this.state.error)
      return <Banner tone="error">This part failed to render: {this.state.error.message}</Banner>;
    return this.props.children;
  }
}
