export function renderJobStatus(status: string) {
  switch (status) {
    case "succeeded":
      return "已完成";
    case "running":
      return "执行中";
    case "deferred":
      return "等待接入";
    case "failed":
      return "失败";
    default:
      return status;
  }
}

export function renderJobItemStatus(status: string) {
  switch (status) {
    case "succeeded":
      return "已完成";
    case "running":
      return "执行中";
    case "deferred":
      return "等待接入";
    case "failed_business":
      return "业务失败";
    case "failed_system":
      return "系统失败";
    case "quota_skipped":
      return "次数不足";
    default:
      return status;
  }
}
