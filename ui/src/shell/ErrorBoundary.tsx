// The last line of defence: a render error shows what broke instead of an
// empty window, and the window can be reloaded from here.

import { Component, type ReactNode } from "react";
import { Button } from "../components/ui";

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error) {
    console.error(error);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div role="alert" className="flex h-screen flex-col items-center justify-center gap-3 bg-surface p-6 text-center">
        <p className="text-base font-medium text-fg">SpiderLens hit an error and stopped drawing this window.</p>
        <pre className="max-w-xl whitespace-pre-wrap text-sm text-fg-2">{error.message || String(error)}</pre>
        <Button onClick={() => window.location.reload()}>Reload window</Button>
      </div>
    );
  }
}
