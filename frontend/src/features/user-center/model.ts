import { getMyProfile, getMyQuota, listMyJobs } from "../../shared/api/me.ts";
import type { CurrentUserProfile } from "../../shared/api/me.ts";
import type { PlatformJobSummary, PlatformQuotaView } from "../../shared/api/platform.ts";
import { resolveDeferredExecutionSummary, type DeferredExecutionSummary } from "../../shared/deferredExecution.ts";

export interface UserCenterJobSummary extends PlatformJobSummary {
  deferredSummary: DeferredExecutionSummary | null;
}

export interface UserCenterOverview {
  profile: CurrentUserProfile;
  quota: PlatformQuotaView;
  jobs: UserCenterJobSummary[];
}

async function enrichUserCenterJob(job: PlatformJobSummary): Promise<UserCenterJobSummary> {
  const deferredReasonCode = String(job.deferred_reason_code ?? "").trim();
  const deferredReasonMessage = String(job.deferred_reason_message ?? "").trim();
  return {
    ...job,
    deferredSummary: deferredReasonCode
      ? resolveDeferredExecutionSummary(deferredReasonCode, deferredReasonMessage)
      : null,
  };
}

export async function loadUserCenterOverview(): Promise<UserCenterOverview> {
  const [profile, quota, jobs] = await Promise.all([getMyProfile(), getMyQuota(), listMyJobs()]);

  return {
    profile,
    quota,
    jobs: await Promise.all(jobs.map(enrichUserCenterJob)),
  };
}
