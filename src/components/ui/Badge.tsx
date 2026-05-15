import type { ReactNode } from "react";

type Variant = "ok" | "warn" | "error" | "muted" | "accent" | "running";

interface BadgeProps {
  variant?: Variant;
  children: ReactNode;
  title?: string;
  className?: string;
}

export function Badge({ variant = "muted", children, title, className }: BadgeProps) {
  return (
    <span
      className={`badge badge-${variant} ${className ?? ""}`}
      title={title}
    >
      {children}
    </span>
  );
}
