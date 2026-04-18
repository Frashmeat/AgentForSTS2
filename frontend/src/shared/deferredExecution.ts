import type { PlatformJobEventSummary } from "./api/platform.ts";

export interface DeferredExecutionSummary {
  reasonCode: string;
  reasonMessage: string;
  title: string;
  description: string;
  detail: string;
  shortLabel: string;
  alertMessage: string;
}

export interface DeferredExecutionNotice {
  event: PlatformJobEventSummary;
  summary: DeferredExecutionSummary;
}

function fallbackDetail(reasonCode: string) {
  switch (reasonCode) {
    case "local_project_root_required":
      return "当前任务仍依赖本地项目目录，服务器模式暂时还不能直接继续执行。";
    case "workflow_not_registered":
      return "当前任务类型尚未接入服务器执行器。";
    default:
        return "当前任务暂未进入真实服务器执行。";
  }
}

export function resolveDeferredExecutionSummary(
  reasonCode: string,
  reasonMessage: string,
): DeferredExecutionSummary {
  const detail = reasonMessage || fallbackDetail(reasonCode);
  switch (reasonCode) {
    case "local_project_root_required":
      return {
        reasonCode,
        reasonMessage,
        title: "当前任务仍依赖本地项目目录",
        description: "服务器模式暂时还不能直接消费用户本机的 `project_root`，因此这次开始后只创建了执行记录，没有进入真实服务器 runner。",
        detail,
        shortLabel: "依赖本地目录",
        alertMessage: `任务已创建，但当前仍依赖本地项目目录，服务器还不能直接继续执行。\n${detail}`,
      };
    case "workflow_not_registered":
      return {
        reasonCode,
        reasonMessage,
        title: "当前任务类型尚未接入服务器执行器",
        description: "后端已经记录了这次开始请求，但当前 web runtime 还没有为该任务注册可直接执行的服务器 workflow。",
        detail,
        shortLabel: "等待服务器接入",
        alertMessage: `任务已创建，但当前任务类型尚未接入服务器执行器。\n${detail}`,
      };
    default:
      return {
        reasonCode,
        reasonMessage,
        title: "当前任务暂未进入服务器执行",
        description: "后端已记录本次开始请求，但暂时没有进入真实服务器执行链。",
        detail,
        shortLabel: "暂未进入执行",
        alertMessage: `任务已创建，但当前没有进入真实服务器执行。\n${detail}`,
      };
  }
}

export function readDeferredExecutionNotice(
  events: PlatformJobEventSummary[],
): DeferredExecutionNotice | null {
  const deferredEvent = [...events].reverse().find(event => event.event_type === "ai_execution.deferred");
  if (!deferredEvent) {
    return null;
  }
  return {
    event: deferredEvent,
    summary: resolveDeferredExecutionSummary(
      String(deferredEvent.payload.reason_code ?? ""),
      String(deferredEvent.payload.reason_message ?? ""),
    ),
  };
}
