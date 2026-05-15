import type { ReactNode } from "react";

interface KVListProps {
  children: ReactNode;
  variant?: "default" | "narrow" | "wide";
  className?: string;
}

export function KVList({ children, variant = "default", className }: KVListProps) {
  const klass = variant === "narrow" ? "kv-list-narrow" : variant === "wide" ? "kv-list-wide" : "";
  return <dl className={`kv-list ${klass} ${className ?? ""}`}>{children}</dl>;
}

interface KVProps {
  k: ReactNode;
  children: ReactNode;
}

export function KV({ k, children }: KVProps) {
  return (
    <>
      <dt className="kv-key">{k}</dt>
      <dd className="kv-val">{children}</dd>
    </>
  );
}
