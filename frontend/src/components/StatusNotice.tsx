import type { ReactNode } from "react";
import { AlertTriangle, CheckCircle2, Info, Loader2, OctagonAlert } from "lucide-react";

export type StatusNoticeTone = "info" | "success" | "warning" | "error";

export interface StatusNoticeProps {
  title: string;
  message?: string;
  tone?: StatusNoticeTone;
  details?: string[];
  actions?: ReactNode;
  progress?: number;
  indeterminate?: boolean;
  floating?: boolean;
}

function resolveToneClasses(tone: StatusNoticeTone) {
  switch (tone) {
    case "success":
      return {
        shell: "border-emerald-200 bg-emerald-50/95 text-emerald-950 shadow-[0_18px_50px_rgba(5,150,105,0.16)]",
        icon: "text-emerald-600",
        title: "text-emerald-900",
        message: "text-emerald-800",
        detail: "text-emerald-700/90",
        track: "bg-emerald-100",
        fill: "bg-emerald-500",
      };
    case "warning":
      return {
        shell: "border-amber-200 bg-amber-50/95 text-amber-950 shadow-[0_18px_50px_rgba(217,119,6,0.16)]",
        icon: "text-amber-600",
        title: "text-amber-900",
        message: "text-amber-800",
        detail: "text-amber-700/90",
        track: "bg-amber-100",
        fill: "bg-amber-500",
      };
    case "error":
      return {
        shell: "border-rose-200 bg-rose-50/95 text-rose-950 shadow-[0_18px_50px_rgba(225,29,72,0.16)]",
        icon: "text-rose-600",
        title: "text-rose-900",
        message: "text-rose-800",
        detail: "text-rose-700/90",
        track: "bg-rose-100",
        fill: "bg-rose-500",
      };
    default:
      return {
        shell: "border-sky-200 bg-sky-50/95 text-sky-950 shadow-[0_18px_50px_rgba(14,165,233,0.14)]",
        icon: "text-sky-600",
        title: "text-sky-900",
        message: "text-sky-800",
        detail: "text-sky-700/90",
        track: "bg-sky-100",
        fill: "bg-sky-500",
      };
  }
}

function NoticeIcon({ tone, indeterminate }: { tone: StatusNoticeTone; indeterminate?: boolean }) {
  const iconCls = `h-4 w-4 ${resolveToneClasses(tone).icon}`;
  if (indeterminate) {
    return <Loader2 className={`${iconCls} animate-spin`} />;
  }
  switch (tone) {
    case "success":
      return <CheckCircle2 className={iconCls} />;
    case "warning":
      return <AlertTriangle className={iconCls} />;
    case "error":
      return <OctagonAlert className={iconCls} />;
    default:
      return <Info className={iconCls} />;
  }
}

export function StatusNotice({
  title,
  message,
  tone = "info",
  details = [],
  actions,
  progress,
  indeterminate = false,
  floating = false,
}: StatusNoticeProps) {
  const toneCls = resolveToneClasses(tone);

  return (
    <section
      className={`rounded-2xl border px-4 py-3 backdrop-blur-sm ${toneCls.shell} ${
        floating ? "w-[min(24rem,calc(100vw-2rem))]" : ""
      }`}
    >
      <div className="flex items-start gap-3">
        <div className="mt-0.5 shrink-0">
          <NoticeIcon tone={tone} indeterminate={indeterminate} />
        </div>
        <div className="min-w-0 flex-1 space-y-1.5">
          <p className={`text-sm font-semibold ${toneCls.title}`}>{title}</p>
          {message ? <p className={`text-sm leading-5 ${toneCls.message}`}>{message}</p> : null}
          {typeof progress === "number" || indeterminate ? (
            <div className={`h-1.5 overflow-hidden rounded-full ${toneCls.track}`}>
              {indeterminate ? (
                <div
                  className={`h-full w-1/3 rounded-full ${toneCls.fill} animate-[knowledge-progress-indeterminate_1.2s_ease-in-out_infinite]`}
                />
              ) : (
                <div
                  className={`h-full rounded-full transition-[width] duration-300 ease-out ${toneCls.fill}`}
                  style={{ width: `${Math.max(0, Math.min(progress ?? 0, 100))}%` }}
                />
              )}
            </div>
          ) : null}
          {details.length > 0 ? (
            <div className="space-y-1">
              {details.map((detail, index) => (
                <p key={`${detail}-${index}`} className={`text-xs leading-5 ${toneCls.detail}`}>
                  {detail}
                </p>
              ))}
            </div>
          ) : null}
          {actions ? <div className="pointer-events-auto pt-1">{actions}</div> : null}
        </div>
      </div>
    </section>
  );
}

export type StatusNoticeItem = StatusNoticeProps & { id: string };

interface StatusNoticeStackProps {
  notices: StatusNoticeItem[];
}

export function StatusNoticeStack({ notices }: StatusNoticeStackProps) {
  if (notices.length === 0) {
    return null;
  }

  return (
    <div className="pointer-events-none fixed right-4 top-4 z-[70] flex flex-col items-end gap-3">
      {notices.map((notice) => (
        <div key={notice.id} className="pointer-events-auto">
          <StatusNotice {...notice} floating />
        </div>
      ))}
    </div>
  );
}
