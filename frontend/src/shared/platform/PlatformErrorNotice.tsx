import type { PlatformErrorView } from "./errors.ts";

interface PlatformErrorNoticeProps {
  error: PlatformErrorView;
}

export function platformErrorNoticeTitle(error: PlatformErrorView): string {
  return `生成失败：${error.title}`;
}

export function platformErrorNoticeDetails(error: PlatformErrorView): string[] {
  return [error.reasonCode ? `诊断码：${error.reasonCode}` : "", error.stepId ? `步骤：${error.stepId}` : ""].filter(
    Boolean,
  );
}

export function PlatformErrorNotice({ error }: PlatformErrorNoticeProps) {
  return (
    <div className="space-y-1">
      <p className="text-sm font-semibold">{platformErrorNoticeTitle(error)}</p>
      <p className="text-sm leading-5">{error.message}</p>
      {platformErrorNoticeDetails(error).map((detail) => (
        <p key={detail} className="text-xs opacity-80">
          {detail}
        </p>
      ))}
    </div>
  );
}
