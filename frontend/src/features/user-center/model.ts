import { getMyProfile, getMyQuota, listMyJobs } from "../../shared/api/me.ts";
import type { CurrentUserProfile } from "../../shared/api/me.ts";
import type { PlatformJobSummary, PlatformQuotaView } from "../../shared/api/platform.ts";

export interface UserCenterOverview {
  profile: CurrentUserProfile;
  quota: PlatformQuotaView;
  jobs: PlatformJobSummary[];
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
    jobs,
  };
}
