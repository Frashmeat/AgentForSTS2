import type {
  ItemDefinition,
  ItemResourceBinding,
  ResourceAsset,
  ResourceCatalog,
  ResourceRoleDescriptor,
} from "@/services/tauriApi";

export function workbenchRoles(
  catalog: ResourceCatalog,
  requiredRoles: string[],
): ResourceRoleDescriptor[] {
  const byId = new Map(catalog.roles.map((role) => [role.id, role]));
  const included = new Set(requiredRoles);
  const visit = (roleId: string) => {
    const role = byId.get(roleId);
    if (role?.source.kind === "derived" && !included.has(role.source.sourceRole)) {
      included.add(role.source.sourceRole);
      visit(role.source.sourceRole);
    }
  };
  requiredRoles.forEach(visit);
  return catalog.roles.filter((role) => included.has(role.id));
}

export function candidatesForRole(
  assets: ResourceAsset[],
  roleId: string,
): ResourceAsset[] {
  return assets.filter((asset) => asset.logicalRole === roleId);
}

export function bindSelectedResource(
  definition: ItemDefinition,
  logicalRole: string,
  asset: ResourceAsset,
  version: string,
): ItemDefinition {
  if (
    asset.logicalRole !== logicalRole ||
    asset.selectedVersion !== version ||
    !asset.versions.some((candidate) => candidate.id === version)
  ) {
    throw new Error("resource binding does not match an explicitly selected candidate");
  }
  const binding: ItemResourceBinding = {
    resourceId: asset.resourceId,
    selectedVersion: version,
  };
  return {
    ...definition,
    resourceBindings: {
      ...definition.resourceBindings,
      [logicalRole]: binding,
    },
  };
}

export function resourceBindingIssues(
  definition: ItemDefinition,
  requiredRoles: string[],
  catalog: ResourceCatalog,
  assets: ResourceAsset[],
): string[] {
  const issues: string[] = [];
  const required = new Set(requiredRoles);
  const roleSpecs = new Map(catalog.roles.map((role) => [role.id, role]));
  for (const role of requiredRoles) {
    const binding = definition.resourceBindings[role];
    if (!binding) {
      issues.push(`${role}: select a resource candidate.`);
      continue;
    }
    const asset = assets.find((candidate) => candidate.resourceId === binding.resourceId);
    const version = asset?.versions.find((candidate) => candidate.id === binding.selectedVersion);
    const spec = roleSpecs.get(role);
    if (
      !asset ||
      !version ||
      !spec ||
      asset.logicalRole !== role ||
      asset.selectedVersion !== binding.selectedVersion ||
      !spec.mediaTypes.includes(version.blob.mediaType) ||
      version.blob.width !== spec.width ||
      version.blob.height !== spec.height ||
      (spec.requireAlpha && !version.blob.hasAlpha)
    ) {
      issues.push(`${role}: saved binding is stale or does not match the Pack shape.`);
    }
  }
  for (const role of Object.keys(definition.resourceBindings)) {
    if (!required.has(role)) issues.push(`${role}: binding is not declared by this item type.`);
  }
  return issues;
}
