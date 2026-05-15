import type { ReactNode } from "react";

interface CardProps {
  eyebrow?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
  className?: string;
}

export function Card({
  eyebrow,
  title,
  subtitle,
  actions,
  children,
  className,
}: CardProps) {
  return (
    <section className={`card-shell ${className ?? ""}`}>
      {(eyebrow || title || subtitle || actions) && (
        <div className="card-header">
          <div className="card-header-text">
            {eyebrow && <span className="eyebrow-label">{eyebrow}</span>}
            <h2 className="card-title">{title}</h2>
            {subtitle && <p className="card-subtitle">{subtitle}</p>}
          </div>
          {actions && <div className="card-actions">{actions}</div>}
        </div>
      )}
      {children}
    </section>
  );
}

export function CardSection({
  title,
  children,
  className,
}: {
  title?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div className={`card-section ${className ?? ""}`}>
      {title && <h3>{title}</h3>}
      {children}
    </div>
  );
}
