import type { ReactNode } from "react";

interface PageHeroProps {
  eyebrow?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
}

export function PageHero({ eyebrow, title, subtitle, actions }: PageHeroProps) {
  return (
    <header className="page-hero">
      <div className="page-hero-text">
        {eyebrow && <span className="eyebrow-label">{eyebrow}</span>}
        <h1>{title}</h1>
        {subtitle && <p className="page-hero-subtitle">{subtitle}</p>}
      </div>
      {actions && <div className="page-hero-actions">{actions}</div>}
    </header>
  );
}
