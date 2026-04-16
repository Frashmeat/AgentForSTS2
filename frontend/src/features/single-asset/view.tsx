import { useState, type ReactNode } from "react";
import { AlertTriangle, ChevronDown, ChevronUp, Loader2, RotateCcw } from "lucide-react";

import { AgentLog } from "../../components/AgentLog";
import { ApprovalPanel } from "../../components/ApprovalPanel";
import { BuildDeploy } from "../../components/BuildDeploy";
import { KnowledgeStatusBanner } from "../../components/KnowledgeStatusBanner.tsx";
import { type KnowledgeStatus } from "../../shared/api/index.ts";
import { ProjectRootField } from "../../components/ProjectRootField";
import { StageStatus } from "../../components/StageStatus";
import { cn } from "../../lib/utils";
import type { ApprovalRequest } from "../../shared/api/index.ts";
import type { WorkflowLogEntry } from "../../shared/workflowLog.ts";

import { ASSET_TYPES, PRESETS, type AssetType, type PresetOption, type Stage } from "./model";

export interface SingleAssetFeatureViewProps {
  step: number;
  stage: Stage;
  assetType: AssetType;
  assetName: string;
  description: string;
  projectRoot: string;
  images: string[];
  pendingSlots: number;
  promptPreview: string;
  negativePrompt: string;
  promptFallbackWarn: string | null;
  currentPrompt: string;
  showMorePrompt: boolean;
  genLog: string[];
  agentLog: string[];
  agentLogEntries: WorkflowLogEntry[];
  currentAgentModel: string | null;
  flowStageCurrent: string | null;
  flowStageHistory: string[];
  agentStageCurrent: string | null;
  agentStageHistory: string[];
  approvalSummary: string;
  approvalRequests: ApprovalRequest[];
  approvalBusyActionId: string | null;
  errorMessage: string | null;
  errorTraceback: string | null;
  autoMode: boolean;
  imageMode: "ai" | "upload";
  uploadedImageB64: string;
  uploadedImageName: string;
  uploadedImagePreview: string | null;
  dragOver: boolean;
  hasLiveSession: boolean;
  showRecoveredNotice: boolean;
  knowledgeStatus: KnowledgeStatus | null;
  onRestartWorkflow: () => void;
  onAssetTypeChange: (value: AssetType) => void;
  onAssetNameChange: (value: string) => void;
  onDescriptionChange: (value: string) => void;
  onProjectRootChange: (value: string) => void;
  projectCreateBusy: boolean;
  projectCreateMessage: string | null;
  projectCreateError: string | null;
  onCreateProject: () => void;
  onApplyPreset: (preset: PresetOption) => void;
  onStartWorkflow: () => void;
  onReset: () => void;
  onImageModeChange: (value: "ai" | "upload") => void;
  onAutoModeToggle: () => void;
  onPromptPreviewChange: (value: string) => void;
  onNegativePromptChange: (value: string) => void;
  onConfirmPrompt: () => void;
  onSelectImage: (index: number) => void;
  onGenerateMore: () => void;
  onCurrentPromptChange: (value: string) => void;
  onToggleShowMorePrompt: () => void;
  onHandleImageFile: (file: File) => void;
  onDragOverChange: (value: boolean) => void;
  onApprove: (actionId: string) => void;
  onReject: (actionId: string) => void;
  onExecute: (actionId: string) => void;
  onProceedApproval: () => void;
  onRefreshKnowledge: () => void;
  onOpenKnowledgeGuide: () => void;
  onOpenSettings: () => void;
}

export function SingleAssetFeatureView(props: SingleAssetFeatureViewProps) {
  const {
    step,
    stage,
    assetType,
    assetName,
    description,
    projectRoot,
    images,
    pendingSlots,
    promptPreview,
    negativePrompt,
    promptFallbackWarn,
    currentPrompt,
    showMorePrompt,
    genLog,
    agentLog,
    agentLogEntries,
    currentAgentModel,
    flowStageCurrent,
    flowStageHistory,
    agentStageCurrent,
    agentStageHistory,
    approvalSummary,
    approvalRequests,
    approvalBusyActionId,
    errorMessage,
    errorTraceback,
    autoMode,
    imageMode,
    uploadedImageB64,
    uploadedImageName,
    uploadedImagePreview,
    dragOver,
    hasLiveSession,
    showRecoveredNotice,
    knowledgeStatus,
    onRestartWorkflow,
    onAssetTypeChange,
    onAssetNameChange,
    onDescriptionChange,
    onProjectRootChange,
    projectCreateBusy,
    projectCreateMessage,
    projectCreateError,
    onCreateProject,
    onApplyPreset,
    onStartWorkflow,
    onReset,
    onImageModeChange,
    onAutoModeToggle,
    onPromptPreviewChange,
    onNegativePromptChange,
    onConfirmPrompt,
    onSelectImage,
    onGenerateMore,
    onCurrentPromptChange,
    onToggleShowMorePrompt,
    onHandleImageFile,
    onDragOverChange,
    onApprove,
    onReject,
    onExecute,
    onProceedApproval,
    onRefreshKnowledge,
    onOpenKnowledgeGuide,
    onOpenSettings,
  } = props;

  const errorInStep2 = stage === "error" && step <= 2;
  const errorInStep3 = stage === "error" && step > 2;
  const showKnowledgeNotice = knowledgeStatus?.status === "stale" || knowledgeStatus?.status === "missing";
  const isCustomCode = assetType === "custom_code";
  const startDisabled =
    !assetName.trim() ||
    !description.trim() ||
    !projectRoot.trim() ||
    (!isCustomCode && imageMode === "upload" && !uploadedImageB64);

  return (
    <main className="px-6 py-6 grid grid-cols-[minmax(0,1fr)_minmax(0,1.5fr)] gap-5 items-start">
      <Step num={1} title="描述设计" active={step === 0} done={step > 0}>
        <div className="space-y-4">
          {showKnowledgeNotice && (
            <KnowledgeStatusBanner
              status={knowledgeStatus}
              onOpenGuide={onOpenKnowledgeGuide}
              onOpenSettings={onOpenSettings}
            />
          )}
          {showRecoveredNotice && (
            <div className="rounded-lg border border-violet-200 bg-violet-50 px-3 py-2.5 text-xs text-violet-700 space-y-2">
              <p>当前展示的是本地恢复的单资产快照。审批状态会同步后端，但 prompt 确认、选图、补图和继续执行需要重新建立工作流连接。</p>
              <button
                onClick={onRestartWorkflow}
                className="inline-flex items-center gap-1 rounded-md border border-violet-300 px-2.5 py-1 text-xs font-medium text-violet-700 hover:bg-violet-100 transition-colors"
              >
                <RotateCcw size={11} />
                按当前输入重新开始
              </button>
            </div>
          )}
          <div className="space-y-2">
            <label className="text-xs font-medium text-slate-500">资产类型</label>
            <div className="flex gap-2 flex-wrap">
              {ASSET_TYPES.map((item) => (
                <button
                  key={item.value}
                  disabled={step > 0}
                  onClick={() => onAssetTypeChange(item.value)}
                  className={cn(
                    "py-1 px-3 rounded-md border text-sm transition-all",
                    assetType === item.value
                      ? "border-violet-500 bg-violet-50 text-violet-700 font-medium"
                      : "border-slate-200 hover:border-violet-300 text-slate-500 hover:text-slate-700",
                    step > 0 && "opacity-50 cursor-not-allowed"
                  )}
                >
                  {item.label}
                </button>
              ))}
            </div>
            <p className="text-xs text-slate-400">{ASSET_TYPES.find((item) => item.value === assetType)?.imgHint}</p>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <label className="text-xs font-medium text-slate-500">资产名称（英文）</label>
              <input
                value={assetName}
                disabled={step > 0}
                onChange={(event) => onAssetNameChange(event.target.value)}
                placeholder="DarkBlade"
                className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-violet-400 focus:ring-1 focus:ring-violet-100 disabled:opacity-50"
              />
            </div>
            <ProjectRootField
              value={projectRoot}
              disabled={step > 0}
              placeholder="E:/STS2mod"
              showCreateAction={step === 0}
              createActionLabel="创建项目"
              createBusy={projectCreateBusy}
              createMessage={projectCreateMessage}
              createError={projectCreateError}
              onChange={onProjectRootChange}
              onCreateProject={onCreateProject}
            />
          </div>

          {step === 0 && (
            <div className="space-y-1">
              <label className="text-xs font-medium text-slate-500">快速示例</label>
              <div className="flex gap-1.5 flex-wrap">
                {PRESETS.map((preset) => (
                  <button
                    key={preset.label}
                    onClick={() => onApplyPreset(preset)}
                    className="px-2.5 py-1 rounded-md border border-slate-200 text-xs text-slate-500 hover:border-violet-300 hover:text-violet-700 transition-colors"
                  >
                    {preset.label}
                  </button>
                ))}
              </div>
            </div>
          )}

          <div className="space-y-1">
            <label className="text-xs font-medium text-slate-500">设计描述</label>
            <textarea
              value={description}
              disabled={step > 0}
              onChange={(event) => onDescriptionChange(event.target.value)}
              rows={4}
              placeholder="描述这个资产的外观、效果、数值……"
              className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-violet-400 focus:ring-1 focus:ring-violet-100 resize-none disabled:opacity-50"
            />
          </div>

          {step === 0 ? (
            <div className="space-y-3">
              {!isCustomCode && (
                <div className="space-y-2">
                  <label className="text-xs font-medium text-slate-500">图片来源</label>
                  <div className="flex gap-2">
                    {(["ai", "upload"] as const).map((mode) => (
                      <button
                        key={mode}
                        onClick={() => onImageModeChange(mode)}
                        className={cn(
                          "flex-1 py-1.5 rounded-lg border text-xs font-medium transition-all",
                          imageMode === mode
                            ? "border-violet-500 bg-violet-50 text-violet-700"
                            : "border-slate-200 text-slate-500 hover:border-violet-300"
                        )}
                      >
                        {mode === "ai" ? "✦ AI 生图" : "↑ 自定义图片"}
                      </button>
                    ))}
                  </div>

                  {imageMode === "upload" && (
                    <>
                      <div
                        onDragOver={(event) => {
                          event.preventDefault();
                          onDragOverChange(true);
                        }}
                        onDragLeave={() => onDragOverChange(false)}
                        onDrop={(event) => {
                          event.preventDefault();
                          onDragOverChange(false);
                          const file = event.dataTransfer.files[0];
                          if (file) {
                            onHandleImageFile(file);
                          }
                        }}
                        onClick={() => {
                          const input = document.createElement("input");
                          input.type = "file";
                          input.accept = "image/*";
                          input.onchange = (event) => {
                            const file = (event.target as HTMLInputElement).files?.[0];
                            if (file) {
                              onHandleImageFile(file);
                            }
                          };
                          input.click();
                        }}
                        className={cn(
                          "relative rounded-lg border-2 border-dashed cursor-pointer transition-colors overflow-hidden",
                          dragOver ? "border-violet-400 bg-violet-50" : "border-slate-200 hover:border-violet-300 bg-slate-50",
                          uploadedImagePreview ? "h-32" : "h-20"
                        )}
                      >
                        {uploadedImagePreview ? (
                          <>
                            <img src={uploadedImagePreview} alt="preview" className="w-full h-full object-contain" />
                            <div className="absolute inset-0 bg-black/30 flex items-center justify-center opacity-0 hover:opacity-100 transition-opacity">
                              <span className="text-white text-xs font-medium">点击替换</span>
                            </div>
                          </>
                        ) : (
                          <div className="flex flex-col items-center justify-center h-full gap-1">
                            <span className="text-slate-400 text-lg">↑</span>
                            <span className="text-xs text-slate-400">拖拽或点击选择图片</span>
                          </div>
                        )}
                      </div>
                      {uploadedImageName && <p className="text-xs text-slate-400 truncate">{uploadedImageName}</p>}
                    </>
                  )}
                </div>
              )}

              {isCustomCode && (
                <div className="rounded-lg border border-sky-200 bg-sky-50 px-3 py-2.5 text-xs text-sky-700">
                  当前类型不走图像链，会直接进入 Code Agent；服务器模式下则会直接创建文本实现方案任务。
                </div>
              )}

              <button
                onClick={onStartWorkflow}
                disabled={startDisabled}
                className="w-full py-2.5 rounded-lg bg-violet-700 text-white font-bold text-sm hover:bg-violet-800 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
              >
                开始生成
              </button>

              {!isCustomCode && imageMode === "ai" && (
                <label className="flex items-center gap-2 cursor-pointer select-none">
                  <div
                    onClick={onAutoModeToggle}
                    className={cn(
                      "relative w-8 h-4 rounded-full transition-colors shrink-0",
                      autoMode ? "bg-violet-700" : "bg-slate-200"
                    )}
                  >
                    <span
                      className={cn(
                        "absolute top-0.5 w-3 h-3 rounded-full bg-white shadow transition-transform",
                        autoMode ? "translate-x-4" : "translate-x-0.5"
                      )}
                    />
                  </div>
                  <span className="text-xs text-slate-400">自动模式（跳过确认，自动选第 1 张图）</span>
                </label>
              )}
            </div>
          ) : (
            <button
              onClick={onReset}
              className="w-full py-2 rounded-lg border border-slate-200 text-slate-400 hover:text-violet-700 hover:border-violet-300 text-sm transition-colors flex items-center justify-center gap-1.5"
            >
              <RotateCcw size={13} />
              重新开始
            </button>
          )}
        </div>
      </Step>

      <div className="space-y-4">
        <Step num={2} title="生成图像" active={step >= 1 && step <= 3} done={step > 3}>
          {step === 0 && <p className="text-sm text-slate-300">等待开始…</p>}

          {step === 1 && (
            <div className="space-y-3">
              {promptFallbackWarn && (
                <div className="flex items-start gap-2 rounded-lg border border-yellow-300 bg-yellow-50 px-3 py-2">
                  <span className="text-yellow-600 font-bold text-xs shrink-0">⚠ AI 优化失败</span>
                  <p className="text-xs text-yellow-700 font-mono break-all">{promptFallbackWarn}</p>
                </div>
              )}
              <p className="text-xs font-medium text-slate-500">AI 生成的图像提示词（可修改后确认）</p>
              <textarea
                value={promptPreview}
                onChange={(event) => onPromptPreviewChange(event.target.value)}
                rows={6}
                className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-700 focus:outline-none focus:border-violet-400 focus:ring-1 focus:ring-violet-100 resize-none font-mono"
              />
              {negativePrompt && (
                <>
                  <p className="text-xs font-medium text-slate-500">Negative prompt</p>
                  <textarea
                    value={negativePrompt}
                    onChange={(event) => onNegativePromptChange(event.target.value)}
                    rows={2}
                    className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-700 focus:outline-none focus:border-violet-400 resize-none font-mono"
                  />
                </>
              )}
              <button
                onClick={onConfirmPrompt}
                disabled={!hasLiveSession}
                className="w-full py-2.5 rounded-lg bg-violet-700 text-white font-bold text-sm hover:bg-violet-800 transition-colors disabled:cursor-not-allowed disabled:bg-slate-400"
              >
                确认，开始生图
              </button>
            </div>
          )}

          {step === 2 && !errorInStep2 && (
            <div className="space-y-3">
              <StageStatus current={flowStageCurrent} history={flowStageHistory} />
              {genLog.length > 0 ? (
                <AgentLog lines={genLog} />
              ) : (
                <div className="flex items-center gap-2.5 py-3">
                  <Loader2 size={16} className="text-violet-500 animate-spin" />
                  <span className="text-sm text-slate-400">正在生成图像…</span>
                </div>
              )}
            </div>
          )}

          {errorInStep2 && <ErrorBlock message={errorMessage} traceback={errorTraceback} log={genLog} onReset={onReset} />}

          {step === 3 && (
            <div className="space-y-4">
              <StageStatus current={flowStageCurrent} history={flowStageHistory} />
              {genLog.length > 0 && <AgentLog lines={genLog} />}

              <div className="flex flex-wrap gap-3">
                {images.map((image, index) => (
                  <div
                    key={index}
                    className="group relative rounded-lg overflow-hidden border border-slate-200 bg-slate-100 hover:border-violet-400 transition-colors"
                    style={{ width: images.length === 1 ? "280px" : "200px" }}
                  >
                    <img src={`data:image/png;base64,${image}`} alt={`生成图 ${index + 1}`} className="w-full h-auto block" />
                    <div className="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center">
                      <button
                        disabled={!hasLiveSession}
                        onClick={() => onSelectImage(index)}
                        className="py-1.5 px-4 rounded-lg bg-violet-700 text-white font-bold text-sm hover:bg-violet-600 transition-colors shadow-lg disabled:cursor-not-allowed disabled:bg-slate-400"
                      >
                        用这张
                      </button>
                    </div>
                    <div className="absolute top-1.5 left-1.5 w-5 h-5 rounded-full bg-black/50 text-white text-xs flex items-center justify-center font-bold">
                      {index + 1}
                    </div>
                  </div>
                ))}
                {Array.from({ length: pendingSlots }, (_, index) => (
                  <div
                    key={`pending-${index}`}
                    className="rounded-lg border-2 border-dashed border-slate-200 bg-slate-50 flex items-center justify-center"
                    style={{ width: "200px", minHeight: "150px" }}
                  >
                    <Loader2 size={20} className="text-violet-400 animate-spin" />
                  </div>
                ))}
              </div>

              <div className="space-y-2 pt-2 border-t border-slate-100">
                <button
                  onClick={onToggleShowMorePrompt}
                  className="text-xs text-slate-400 hover:text-violet-700 transition-colors flex items-center gap-1"
                >
                  {showMorePrompt ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
                  {showMorePrompt ? "收起" : "修改提示词"}
                </button>
                {showMorePrompt && (
                  <textarea
                    value={currentPrompt}
                    onChange={(event) => onCurrentPromptChange(event.target.value)}
                    rows={4}
                    className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-700 focus:outline-none focus:border-violet-400 focus:ring-1 focus:ring-violet-100 resize-none font-mono"
                  />
                )}
                <button
                  disabled={!hasLiveSession}
                  onClick={onGenerateMore}
                  className="w-full py-2 rounded-lg border border-violet-300 text-violet-700 font-medium text-sm hover:bg-violet-50 transition-colors disabled:cursor-not-allowed disabled:border-slate-200 disabled:text-slate-400 disabled:bg-slate-50"
                >
                  再来一张
                </button>
              </div>
            </div>
          )}
        </Step>

        <Step num={3} title="Code Agent / 审批" active={(step >= 4 && step <= 5) || errorInStep3} done={stage === "done" && !errorInStep3}>
          {step >= 4 && !errorInStep3 && (
            <div className="space-y-3">
              {stage === "approval_pending" ? (
                <ApprovalPanel
                  summary={approvalSummary}
                  requests={approvalRequests}
                  busyActionId={approvalBusyActionId}
                  onApprove={onApprove}
                  onReject={onReject}
                  onExecute={onExecute}
                  onProceed={onProceedApproval}
                  proceedDisabled={!hasLiveSession}
                />
              ) : (
                <>
                  <AgentLog
                    lines={agentLog}
                    entries={agentLogEntries}
                    currentModel={currentAgentModel}
                    currentStage={agentStageCurrent ?? agentStageHistory[agentStageHistory.length - 1] ?? null}
                    isComplete={stage === "done"}
                    broadcastKind="codegen"
                  />
                </>
              )}
            </div>
          )}

          {errorInStep3 && <ErrorBlock message={errorMessage} traceback={errorTraceback} log={agentLog} onReset={onReset} />}
          {step < 4 && !errorInStep3 && <p className="text-sm text-slate-300">等待选择图片…</p>}
        </Step>

        <Step num={4} title="完成" active={stage === "approval_pending" || stage === "done"} done={false}>
          {stage === "done" ? (
            <div className="space-y-3">
              <p className="text-sm text-green-600 font-medium">✓ Code Agent 完成</p>
              <BuildDeploy projectRoot={projectRoot} onOpenSettings={onOpenSettings} />
              <button
                onClick={onReset}
                className="py-1.5 px-4 rounded-lg border border-slate-200 hover:border-violet-400 text-slate-500 hover:text-violet-700 transition-colors text-sm flex items-center gap-1.5"
              >
                <RotateCcw size={13} />
                创建新资产
              </button>
            </div>
          ) : stage === "error" ? (
            <div className="space-y-3">
              <p className="text-sm text-red-500 font-medium">✗ 构建失败，查看上方错误详情</p>
              <button
                onClick={onReset}
                className="py-1.5 px-4 rounded-lg border border-slate-200 hover:border-red-300 text-slate-500 hover:text-red-500 transition-colors text-sm flex items-center gap-1.5"
              >
                <RotateCcw size={13} />
                重试
              </button>
            </div>
          ) : stage === "approval_pending" ? (
            <div className="space-y-3">
              <p className="text-sm text-violet-700 font-medium">等待审批通过后继续执行</p>
              <button
                onClick={onReset}
                className="py-1.5 px-4 rounded-lg border border-slate-200 hover:border-violet-300 text-slate-500 hover:text-violet-700 transition-colors text-sm flex items-center gap-1.5"
              >
                <RotateCcw size={13} />
                重新开始
              </button>
            </div>
          ) : (
            <p className="text-sm text-slate-300">等待 Code Agent 完成…</p>
          )}
        </Step>
      </div>
    </main>
  );
}

function ErrorBlock({
  message,
  traceback,
  log,
  onReset,
}: {
  message: string | null;
  traceback: string | null;
  log: string[];
  onReset: () => void;
}) {
  const [showTrace, setShowTrace] = useState(false);

  return (
    <div className="space-y-3">
      {log.length > 0 && <AgentLog lines={log} />}

      <div className="rounded-lg border border-red-200 bg-red-50 p-4 space-y-3">
        <div className="flex items-start gap-2.5">
          <AlertTriangle size={16} className="text-red-500 shrink-0 mt-0.5" />
          <div className="space-y-2 flex-1 min-w-0">
            <p className="text-sm font-semibold text-red-700">执行失败</p>
            {message ? (
              <pre className="text-xs text-red-600/90 font-mono whitespace-pre-wrap break-all leading-relaxed bg-red-100/50 rounded p-2.5 max-h-64 overflow-y-auto">
                {message}
              </pre>
            ) : (
              <p className="text-xs text-red-500">未收到错误详情，查看下方 Traceback</p>
            )}
            {traceback && (
              <>
                <button
                  onClick={() => setShowTrace((value) => !value)}
                  className="text-xs text-red-400 hover:text-red-600 transition-colors flex items-center gap-1"
                >
                  {showTrace ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
                  {showTrace ? "收起 Traceback" : "展开 Traceback"}
                </button>
                {showTrace && (
                  <pre className="text-xs text-red-500/80 font-mono whitespace-pre-wrap break-all leading-relaxed bg-red-100/30 rounded p-2.5 max-h-64 overflow-y-auto border border-red-200/50">
                    {traceback}
                  </pre>
                )}
              </>
            )}
          </div>
        </div>
        <button
          onClick={onReset}
          className="w-full py-2 rounded-lg border border-red-200 text-red-600 hover:bg-red-100 text-sm font-medium transition-colors flex items-center justify-center gap-1.5"
        >
          <RotateCcw size={13} />
          重试
        </button>
      </div>
    </div>
  );
}

function Step({
  num,
  title,
  active,
  done,
  children,
}: {
  num: number;
  title: string;
  active: boolean;
  done: boolean;
  children: ReactNode;
}) {
  return (
    <div
      className={cn(
        "rounded-xl border p-5 transition-all",
        active ? "border-violet-300 bg-white shadow-md" : done ? "border-slate-200 bg-white" : "border-slate-100 bg-slate-50"
      )}
    >
      <div className="flex items-center gap-3 mb-4">
        <div
          className={cn(
            "w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold shrink-0",
            active ? "bg-violet-700 text-white" : done ? "bg-violet-100 text-violet-700" : "bg-slate-200 text-slate-400"
          )}
        >
          {done ? "✓" : num}
        </div>
        <h2 className={cn("font-semibold text-sm", active ? "text-slate-800" : done ? "text-violet-700" : "text-slate-400")}>
          {title}
        </h2>
      </div>
      <div className={cn(!active && !done && "opacity-40 pointer-events-none")}>{children}</div>
    </div>
  );
}
