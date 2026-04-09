export type WorkflowLogChannel = "raw" | "stderr" | "system" | "stage";

export interface WorkflowLogEntry {
  text: string;
  source?: string;
  channel?: WorkflowLogChannel;
  model?: string;
}

export interface CodegenBroadcastStep {
  index: number;
  label: string;
  status: "done" | "current" | "pending";
}

export interface CodegenBroadcastView {
  progressLabel: string;
  completedCount: number;
  currentStep: {
    index: number;
    label: string;
  };
  currentStatus: string;
  summaryLines: string[];
  detailLines: string[];
  steps: CodegenBroadcastStep[];
}

const CODEGEN_STEP_LABELS = [
  "理解任务",
  "扫描项目",
  "定位改动点",
  "生成修改方案",
  "写入代码",
  "自检收尾",
] as const;

const CODEGEN_STEP_PATTERNS: Array<{ index: number; patterns: RegExp[] }> = [
  {
    index: 6,
    patterns: [
      /自检/u,
      /验证/u,
      /收尾/u,
      /测试/u,
      /构建成功/u,
      /构建失败/u,
      /完成代码生成/u,
    ],
  },
  {
    index: 5,
    patterns: [
      /写入代码/u,
      /写入文件/u,
      /修改文件/u,
      /开始修改/u,
      /Update File/u,
      /Add File/u,
      /Delete File/u,
      /apply_patch/u,
    ],
  },
  {
    index: 4,
    patterns: [
      /修改方案/u,
      /整理方案/u,
      /整理本轮修改/u,
      /整理补丁/u,
      /补丁/u,
      /方案/u,
      /计划/u,
    ],
  },
  {
    index: 3,
    patterns: [
      /定位/u,
      /锁定/u,
      /改动点/u,
      /入口/u,
      /目标文件/u,
      /关键位置/u,
    ],
  },
  {
    index: 2,
    patterns: [
      /扫描/u,
      /项目结构/u,
      /关键文件/u,
      /读取/u,
      /查看/u,
      /检查/u,
      /工作区/u,
      /workspace/u,
    ],
  },
];

export function appendWorkflowLogEntry(
  entries: WorkflowLogEntry[],
  entry: WorkflowLogEntry,
): WorkflowLogEntry[] {
  const previous = entries[entries.length - 1];
  if (
    previous &&
    previous.source === entry.source &&
    previous.channel === entry.channel &&
    previous.model === entry.model
  ) {
    return [
      ...entries.slice(0, -1),
      {
        ...previous,
        text: `${previous.text}${entry.text}`,
      },
    ];
  }
  return [...entries, entry];
}

export function resolveNextWorkflowModel(
  currentModel: string | null,
  entry: WorkflowLogEntry,
): string | null {
  return entry.model?.trim() ? entry.model : currentModel;
}

export function buildRawWorkflowLogLines(entries: WorkflowLogEntry[]): string[] {
  return entries.map((entry) => entry.text);
}

export function buildPrettyWorkflowLogLines(entries: WorkflowLogEntry[]): string[] {
  const lines: string[] = [];
  for (const entry of entries) {
    const rawText = entry.text ?? "";
    if (!rawText.trim()) {
      continue;
    }
    const text = entry.channel === "stderr" && !rawText.startsWith("[stderr]")
      ? `[stderr] ${rawText}`
      : rawText;
    if (lines[lines.length - 1] === text) {
      continue;
    }
    lines.push(text);
  }
  return lines;
}

function resolveCodegenStepIndex(entries: WorkflowLogEntry[], isComplete: boolean): number {
  if (isComplete) {
    return 6;
  }

  let currentStepIndex = 1;
  for (const entry of entries) {
    const text = `${entry.text ?? ""}`.trim();
    if (!text) {
      continue;
    }
    for (const candidate of CODEGEN_STEP_PATTERNS) {
      if (candidate.patterns.some((pattern) => pattern.test(text))) {
        currentStepIndex = Math.max(currentStepIndex, candidate.index);
        break;
      }
    }
  }
  return currentStepIndex;
}

function buildCodegenCurrentStatus(currentStepIndex: number, isComplete: boolean): string {
  if (isComplete) {
    return "系统已完成代码生成与自检收尾，当前等待后续处理。";
  }

  switch (currentStepIndex) {
    case 1:
      return "系统已接收代码生成任务，当前开始理解需求与上下文。";
    case 2:
      return "系统已完成任务理解，当前正在扫描项目结构与关键文件。";
    case 3:
      return "系统已完成项目扫描，当前正在定位本轮改动点。";
    case 4:
      return "系统已完成项目扫描与改动点定位，当前进入修改方案整理阶段。";
    case 5:
      return "系统已完成方案整理，当前开始写入代码与整理改动结果。";
    case 6:
      return "系统已完成代码写入，当前进入自检与收尾阶段。";
    default:
      return "系统已接收代码生成任务，当前开始理解需求与上下文。";
  }
}

function buildCodegenSummaryLines(currentStepIndex: number, isComplete: boolean): string[] {
  if (isComplete) {
    return [
      "已完成任务理解、项目扫描、改动点定位与代码写入。",
      "当前已结束自检收尾，本轮代码生成流程已经完成。",
      "如需排查细节，可切换到原始输出查看完整流式内容。",
    ];
  }

  switch (currentStepIndex) {
    case 1:
      return [
        "已接收本轮代码生成任务。",
        "当前正结合上下文理解需求与约束。",
        "暂未进入项目扫描与改动点定位阶段。",
      ];
    case 2:
      return [
        "已完成任务接收与上下文准备。",
        "当前正扫描项目结构与关键文件。",
        "暂未进入改动点定位与方案整理阶段。",
      ];
    case 3:
      return [
        "已完成任务理解与项目扫描。",
        "当前正根据项目结构定位本轮改动点。",
        "暂未进入修改方案整理与代码写入阶段。",
      ];
    case 4:
      return [
        "已完成任务理解、项目扫描与改动点定位。",
        "当前正根据上下文整理本轮修改方案。",
        "暂未进入代码写入与自检收尾阶段。",
      ];
    case 5:
      return [
        "已完成任务理解、项目扫描、改动点定位与方案整理。",
        "当前正写入代码并整理改动结果。",
        "暂未进入自检收尾阶段。",
      ];
    case 6:
      return [
        "已完成任务理解、项目扫描、改动点定位、方案整理与代码写入。",
        "当前正进行自检收尾，确认本轮输出是否闭环。",
        "完成后即可切换到原始输出查看完整技术细节。",
      ];
    default:
      return [
        "已接收本轮代码生成任务。",
        "当前正结合上下文理解需求与约束。",
        "暂未进入项目扫描与改动点定位阶段。",
      ];
  }
}

export function buildCodegenBroadcastView(
  entries: WorkflowLogEntry[],
  options: {
    currentStage?: string | null;
    isComplete?: boolean;
  } = {},
): CodegenBroadcastView {
  const isComplete = options.isComplete ?? false;
  const currentStepIndex = resolveCodegenStepIndex(entries, isComplete);
  const completedCount = isComplete ? 6 : currentStepIndex - 1;
  const detailLines = buildPrettyWorkflowLogLines(entries).slice(-4);
  const fallbackDetail = options.currentStage?.trim() ? [options.currentStage.trim()] : [];

  return {
    progressLabel: `${completedCount} / 6 已完成`,
    completedCount,
    currentStep: {
      index: currentStepIndex,
      label: CODEGEN_STEP_LABELS[currentStepIndex - 1],
    },
    currentStatus: buildCodegenCurrentStatus(currentStepIndex, isComplete),
    summaryLines: buildCodegenSummaryLines(currentStepIndex, isComplete),
    detailLines: detailLines.length > 0 ? detailLines : fallbackDetail,
    steps: CODEGEN_STEP_LABELS.map((label, zeroBasedIndex) => {
      const index = zeroBasedIndex + 1;
      let status: CodegenBroadcastStep["status"] = "pending";
      if (isComplete || index < currentStepIndex) {
        status = "done";
      } else if (index === currentStepIndex) {
        status = isComplete ? "done" : "current";
      }
      return {
        index,
        label,
        status,
      };
    }),
  };
}
