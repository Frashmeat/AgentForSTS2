// 含 JSX 的状态图标常量；从 view.tsx 抽出，与纯字符串 view-constants.ts 分开。

import type React from "react";
import { CheckCircle2, Clock, Code2, ImageIcon, Loader2, StopCircle, XCircle } from "lucide-react";

import type { BatchItemStatus as ItemStatus } from "./state.ts";

export const STATUS_ICONS: Record<ItemStatus, React.ReactNode> = {
  pending: <Clock size={14} className="text-slate-300" />,
  img_generating: <Loader2 size={14} className="text-violet-400 animate-spin" />,
  awaiting_selection: <ImageIcon size={14} className="text-violet-500" />,
  approval_pending: <Clock size={14} className="text-violet-500" />,
  code_generating: <Code2 size={14} className="text-blue-400 animate-pulse" />,
  cancelled: <StopCircle size={14} className="text-slate-400" />,
  done: <CheckCircle2 size={14} className="text-green-500" />,
  error: <XCircle size={14} className="text-red-500" />,
};
