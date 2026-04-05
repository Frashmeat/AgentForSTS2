interface PlatformAuthUnavailableNoticeProps {
  title?: string;
  description?: string;
}

export function PlatformAuthUnavailableNotice({
  title = "当前环境未启用平台账号能力",
  description = "这是本机工作站模式。只有接入独立 Web 平台服务后，登录、注册和用户中心才可用。",
}: PlatformAuthUnavailableNoticeProps) {
  return (
    <section className="mx-auto max-w-3xl rounded-3xl border border-slate-200 bg-white p-8 shadow-sm">
      <h1 className="text-2xl font-semibold text-slate-900">{title}</h1>
      <p className="mt-3 text-sm leading-6 text-slate-500">{description}</p>
    </section>
  );
}

export default PlatformAuthUnavailableNotice;
