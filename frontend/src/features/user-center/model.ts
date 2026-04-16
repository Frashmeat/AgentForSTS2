import { getMyProfile, getMyQuota, listMyJobEvents, listMyJobs } from "../../shared/api/me.ts";
import type { CurrentUserProfile } from "../../shared/api/me.ts";
import type { PlatformJobSummary, PlatformQuotaView } from "../../shared/api/platform.ts";
import {
  readDeferredExecutionNotice,
  type DeferredExecutionSummary,
} from "../../shared/deferredExecution.ts";

export interface UserCenterJobSummary extends PlatformJobSummary {
  deferredSummary: DeferredExecutionSummary | null;
}

export interface UserCenterOverview {
  profile: CurrentUserProfile;
  quota: PlatformQuotaView;
  jobs: UserCenterJobSummary[];
}

async function enrichUserCenterJob(job: PlatformJobSummary): Promise<UserCenterJobSummary> {
  if (job.status !== "running") {
    return {
      ...job,
      deferredSummary: null,
    };
  }

  try {
    const deferredNotice = readDeferredExecutionNotice(await listMyJobEvents(job.id));
    return {
      ...job,
      deferredSummary: deferredNotice?.summary ?? null,
    };
  } catch {
    return {
      ...job,
      deferredSummary: null,
    };
  }
}

export async function loadUserCenterOverview(): Promise<UserCenterOverview> {
  const [profile, quota, jobs] = await Promise.all([
    getMyProfile(),
    getMyQuota(),
    listMyJobs(),
  ]);

  return {
    profile,
    quota,
    jobs: await Promise.all(jobs.map(enrichUserCenterJob)),
  };
}
