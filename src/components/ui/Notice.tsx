import type { ReactNode } from "react";

type Variant = "warn" | "error" | "ok" | "muted";

interface NoticeProps {
  variant?: Variant;
  title?: ReactNode;
  children?: ReactNode;
  className?: string;
}

export function Notice({ variant = "warn", title, children, className }: NoticeProps) {
  return (
    <div className={`notice notice-${variant} ${className ?? ""}`}>
      {title && <p className="notice-title">{title}</p>}
      {children && <div className="notice-body">{children}</div>}
    </div>
  );
}
