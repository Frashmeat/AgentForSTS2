// 卡片级错误边界：单个卡片崩溃不影响其他卡片。
// 在 Dashboard / DevTools 等页面中，每个 Card 外包裹一层。

import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  /** 错误时显示的名称，用于定位是哪个卡片炸了。 */
  label?: string;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <section
          className="card-shell"
          style={{ borderColor: "var(--accent)", borderWidth: "2px" }}
        >
          <div className="card-header">
            <div className="card-header-text">
              <span className="eyebrow-label">error boundary</span>
              <h2 className="card-title">
                {this.props.label ?? "Card"} crashed
              </h2>
            </div>
          </div>
          <pre
            className="pre-block mt-2"
            style={{ fontSize: "12px", color: "var(--accent)" }}
          >
            {this.state.error.message}
          </pre>
        </section>
      );
    }
    return this.props.children;
  }
}
