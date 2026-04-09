export type ReviewStrictness = "efficient" | "balanced" | "strict";

export type BatchStage =
  | "input"
  | "planning"
  | "review_items"
  | "review_bundles"
  | "executing"
  | "done"
  | "error";

export type ItemStatus =
  | "pending"
  | "img_generating"
  | "awaiting_selection"
  | "approval_pending"
  | "code_generating"
  | "done"
  | "error";
