import { useRef, useState } from "react";
import { Loader2, RotateCcw, Search, Wrench } from "lucide-react";

import { AgentLog } from "../../components/AgentLog";
import { BuildDeploy } from "../../components/BuildDeploy";
import { KnowledgeStatusBanner } from "../../components/KnowledgeStatusBanner.tsx";
import { ProjectRootField } from "../../components/ProjectRootField";
import { StageStatus } from "../../components/StageStatus";
import { useDefaultProjectRoot } from "../../shared/useDefaultProjectRoot.ts";
import { useProjectCreation } from "../../shared/useProjectCreation.ts";
import {
  appendWorkflowLogEntry,
  resolveNextWorkflowModel,
  type WorkflowLogEntry,
} from "../../shared/workflowLog.ts";
import { useResolvedWorkspaceFeatureProps } from "../workspace/WorkspaceContext.tsx";
import type { WorkspaceFeatureAdapterProps } from "../workspace/types.ts";
import {
  createModEditorAnalysisController,
  createModEditorModifyController,
  type ModEditorAnalysisSocketLike,
  type ModEditorModifySocketLike,
} from "./controller.ts";

type AnalyzeStage = "idle" | "scanning" | "streaming" | "done" | "error";
type ModifyStage = "idle" | "running" | "done" | "error";

export function ModEditorFeatureView({
  onRequestExecution,
  knowledgeStatus,
  onOpenKnowledgeGuide,
  onOpenSettings,
}: WorkspaceFeatureAdapterProps) {
  const {
    onRequestExecution: resolvedRequestExecution,
    knowledgeStatus: resolvedKnowledgeStatus,
    onOpenKnowledgeGuide: resolvedOpenKnowledgeGuide,
    onOpenSettings: resolvedOpenSettings,
  } = useResolvedWorkspaceFeatureProps({
    onRequestExecution,
    knowledgeStatus,
    onOpenKnowledgeGuide,
    onOpenSettings,
  });
  const [projectRoot, setProjectRoot] = useState("");
  const {
    projectCreateBusy,
    projectCreateMessage,
    projectCreateError,
    clearProjectCreationFeedback,
    createProjectAtRoot,
  } = useProjectCreation({
    onProjectCreated: setProjectRoot,
  });

  useDefaultProjectRoot({
    setProjectRoot,
  });

  const [analyzeStage, setAnalyzeStage] = useState<AnalyzeStage>("idle");
  const [scanFiles, setScanFiles] = useState<number | null>(null);
  const [analysisChunks, setAnalysisChunks] = useState<string[]>([]);
  const [analysisEntries, setAnalysisEntries] = useState<WorkflowLogEntry[]>([]);
  const [analysisCurrentModel, setAnalysisCurrentModel] = useState<string | null>(null);
  const [analysisCurrentStage, setAnalysisCurrentStage] = useState<string | null>(null);
  const [analysisStageHistory, setAnalysisStageHistory] = useState<string[]>([]);
  const [analysisErrorMessage, setAnalysisErrorMessage] = useState<string | null>(null);
  const analyzeWsRef = useRef<ModEditorAnalysisSocketLike | null>(null);

  const [modRequest, setModRequest] = useState("");
  const [modifyStage, setModifyStage] = useState<ModifyStage>("idle");
  const [agentLog, setAgentLog] = useState<string[]>([]);
  const [modifyEntries, setModifyEntries] = useState<WorkflowLogEntry[]>([]);
  const [modifyCurrentModel, setModifyCurrentModel] = useState<string | null>(null);
  const [modifyCurrentStage, setModifyCurrentStage] = useState<string | null>(null);
  const [modifyStageHistory, setModifyStageHistory] = useState<string[]>([]);
  const [modifyErrorMessage, setModifyErrorMessage] = useState<string | null>(null);
  const modifyWsRef = useRef<ModEditorModifySocketLike | null>(null);

  const analysisController = createModEditorAnalysisController({
    closeAnalysisSocket() {
      analyzeWsRef.current?.close();
    },
    setAnalysisSocket(socket) {
      analyzeWsRef.current = socket;
    },
    clearProjectCreationFeedback,
    startAnalysis() {
      setAnalyzeStage("scanning");
      setScanFiles(null);
      setAnalysisChunks([]);
      setAnalysisEntries([]);
      setAnalysisCurrentModel(null);
      setAnalysisCurrentStage(null);
      setAnalysisStageHistory([]);
      setAnalysisErrorMessage(null);
    },
    applyAnalysisStageMessage(message) {
      setAnalysisCurrentStage(message);
      setAnalysisStageHistory((previous) =>
        previous[previous.length - 1] === message ? previous : [...previous, message]
      );
    },
    applyAnalysisScanInfo(files) {
      setScanFiles(files);
      setAnalyzeStage("streaming");
    },
    appendAnalysisChunk(chunk, source, channel, model) {
      setAnalysisChunks((previous) => [...previous, chunk]);
      const entry: WorkflowLogEntry = { text: chunk, source, channel, model };
      setAnalysisEntries((previous) => appendWorkflowLogEntry(previous, entry));
      setAnalysisCurrentModel((previous) => resolveNextWorkflowModel(previous, entry));
    },
    completeAnalysis() {
      setAnalyzeStage("done");
    },
    failAnalysis(message) {
      setAnalysisErrorMessage(message);
      setAnalyzeStage("error");
    },
    resetAnalysis() {
      setAnalyzeStage("idle");
      setScanFiles(null);
      setAnalysisChunks([]);
      setAnalysisEntries([]);
      setAnalysisCurrentModel(null);
      setAnalysisCurrentStage(null);
      setAnalysisStageHistory([]);
      setAnalysisErrorMessage(null);
    },
  });

  const modifyController = createModEditorModifyController({
    closeModifySocket() {
      modifyWsRef.current?.close();
    },
    setModifySocket(socket) {
      modifyWsRef.current = socket;
    },
    clearProjectCreationFeedback,
    startModify() {
      setModifyStage("running");
      setAgentLog([]);
      setModifyEntries([]);
      setModifyCurrentModel(null);
      setModifyCurrentStage(null);
      setModifyStageHistory([]);
      setModifyErrorMessage(null);
    },
    applyModifyStageMessage(message) {
      setModifyCurrentStage(message);
      setModifyStageHistory((previous) =>
        previous[previous.length - 1] === message ? previous : [...previous, message]
      );
    },
    appendModifyLog(line, source, channel, model) {
      setAgentLog((previous) => [...previous, line]);
      const entry: WorkflowLogEntry = { text: line, source, channel, model };
      setModifyEntries((previous) => appendWorkflowLogEntry(previous, entry));
      setModifyCurrentModel((previous) => resolveNextWorkflowModel(previous, entry));
    },
    completeModify(success) {
      setAgentLog((previous) => [...previous, success ? "✓ 修改完成！" : "✗ 修改失败"]);
      const entry: WorkflowLogEntry = {
        text: success ? "✓ 修改完成！" : "✗ 修改失败",
        source: "workflow",
        channel: "system",
      };
      setModifyEntries((previous) => appendWorkflowLogEntry(previous, entry));
      setModifyStage("done");
    },
    failModify(message) {
      setModifyErrorMessage(message);
      setModifyStage("error");
    },
    resetModify() {
      setModifyStage("idle");
      setAgentLog([]);
      setModifyEntries([]);
      setModifyCurrentModel(null);
      setModifyCurrentStage(null);
      setModifyStageHistory([]);
      setModifyErrorMessage(null);
    },
  });

  const analysisText = analysisChunks.join("");
  const isAnalyzing = analyzeStage === "scanning" || analyzeStage === "streaming";
  const isModifying = modifyStage === "running";

  return (
    <div className="space-y-5">
      <KnowledgeStatusBanner
        status={resolvedKnowledgeStatus}
        onOpenGuide={resolvedOpenKnowledgeGuide}
        onOpenSettings={resolvedOpenSettings}
      />
      <div className="workspace-surface rounded-2xl p-5 space-y-4">
        <ProjectRootField
          value={projectRoot}
          placeholder="E:/STS2mod/testscenario/MyMod"
          createActionLabel="创建项目"
          createBusy={projectCreateBusy}
          createMessage={projectCreateMessage}
          createError={projectCreateError}
          onChange={setProjectRoot}
          onCreateProject={() => { void createProjectAtRoot(projectRoot).catch(() => {}); }}
        />
      </div>

      <div className="workspace-surface-strong rounded-2xl p-5 space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Search size={15} className="text-[var(--workspace-accent)]" />
            <h3 className="font-semibold text-[var(--workspace-accent-strong)] text-sm">分析 Mod 内容</h3>
          </div>
          {analyzeStage !== "idle" && (
            <button
              onClick={() => {
                analysisController.reset();
              }}
              className="text-xs text-slate-400 hover:text-slate-600 flex items-center gap-1 transition-colors"
            >
              <RotateCcw size={11} /> 重新分析
            </button>
          )}
        </div>

        {analyzeStage === "idle" && (
          <button
            onClick={() => {
              void analysisController.run(projectRoot);
            }}
            disabled={!projectRoot.trim()}
            className="w-full py-2 rounded-lg border border-violet-300 text-violet-700 font-medium text-sm hover:bg-violet-50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center justify-center gap-2"
          >
            <Search size={14} />
            分析 Mod
          </button>
        )}

        {isAnalyzing && (
          <div className="space-y-3">
            <div className="flex items-center gap-2 py-1">
              <Loader2 size={14} className="text-violet-500 animate-spin" />
              <span className="text-sm text-slate-400">
                {analyzeStage === "scanning" ? "正在扫描源码文件…" : `已扫描 ${scanFiles} 个文件，AI 分析中…`}
              </span>
            </div>
            <StageStatus current={analysisCurrentStage} history={analysisStageHistory} />
          </div>
        )}

        {(analysisText || analysisErrorMessage) && (
          <div className="space-y-2">
            {analysisErrorMessage ? (
              <pre className="text-xs text-red-600 font-mono whitespace-pre-wrap">{analysisErrorMessage}</pre>
            ) : (
              <AgentLog
                lines={analysisChunks}
                entries={analysisEntries}
                currentModel={analysisCurrentModel}
              />
            )}
          </div>
        )}
      </div>

      <div className="workspace-surface-strong rounded-2xl p-5 space-y-4">
        <div className="flex items-center gap-2">
          <Wrench size={15} className="text-[var(--workspace-accent)]" />
          <h3 className="font-semibold text-[var(--workspace-accent-strong)] text-sm">修改 Mod</h3>
        </div>

        <div className="space-y-1">
          <label className="text-xs font-medium text-slate-500">描述要做什么改动</label>
          <textarea
            value={modRequest}
            onChange={(event) => setModRequest(event.target.value)}
            disabled={isModifying}
            rows={4}
            placeholder={"例如：\n把 DarkBlade 的伤害从 8 改成 12，升级后改成 16\n或者：给 FangedGrimoire 增加一个条件，只有血量低于50%时才触发"}
            className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-violet-400 focus:ring-1 focus:ring-violet-100 resize-none disabled:opacity-50"
          />
        </div>

        {modifyStage === "idle" || modifyStage === "done" || modifyStage === "error" ? (
          <div className="flex gap-2">
            <button
              onClick={() => {
                const executeLocal = () => {
                  void modifyController.run(projectRoot, modRequest, analysisText);
                };
                if (!resolvedRequestExecution) {
                  executeLocal();
                  return;
                }
                resolvedRequestExecution({
                  title: "执行 Mod 修改",
                  tab: "edit",
                  jobType: "mod_edit",
                  createdFrom: "mod_editor",
                  inputSummary: modRequest.trim(),
                  requiresCodeAgent: true,
                  requiresImageAi: false,
                  items: [
                    {
                      item_type: "mod_edit",
                      input_summary: modRequest.trim(),
                      input_payload: {
                        project_root: projectRoot.trim(),
                        mod_request: modRequest.trim(),
                        analysis_excerpt: analysisText.slice(0, 1000),
                      },
                    },
                  ],
                  runLocal: executeLocal,
                });
              }}
              disabled={!projectRoot.trim() || !modRequest.trim()}
              className="flex-1 py-2.5 rounded-lg bg-violet-700 text-white font-bold text-sm hover:bg-violet-800 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              执行修改
            </button>
            {(modifyStage === "done" || modifyStage === "error") && (
              <button
                onClick={() => {
                  modifyController.reset();
                }}
                className="py-2.5 px-4 rounded-lg border border-slate-200 text-slate-400 hover:text-slate-600 text-sm transition-colors flex items-center gap-1.5"
              >
                <RotateCcw size={13} /> 重试
              </button>
            )}
          </div>
        ) : (
          <div className="space-y-3">
            <div className="flex items-center gap-2 py-1">
              <Loader2 size={14} className="text-violet-500 animate-spin" />
              <span className="text-sm text-slate-400">Code Agent 执行中…</span>
            </div>
            <StageStatus current={modifyCurrentStage} history={modifyStageHistory} />
          </div>
        )}

        {(agentLog.length > 0 || modifyErrorMessage) && (
          <div className="space-y-2">
            {modifyErrorMessage && <pre className="text-xs text-red-600 font-mono whitespace-pre-wrap">{modifyErrorMessage}</pre>}
            {agentLog.length > 0 && (
              <AgentLog lines={agentLog} entries={modifyEntries} currentModel={modifyCurrentModel} />
            )}
          </div>
        )}

        {modifyStage === "done" && <BuildDeploy projectRoot={projectRoot} />}
      </div>
    </div>
  );
}
