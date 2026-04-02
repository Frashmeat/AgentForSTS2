import { useRef, useState } from "react";
import { Loader2, RotateCcw, Search, Wrench } from "lucide-react";

import { AgentLog } from "../../components/AgentLog";
import { BuildDeploy } from "../../components/BuildDeploy";
import { ProjectRootField } from "../../components/ProjectRootField";
import { StageStatus } from "../../components/StageStatus";
import { useDefaultProjectRoot } from "../../shared/useDefaultProjectRoot.ts";
import { useProjectCreation } from "../../shared/useProjectCreation.ts";
import {
  createModEditorAnalysisController,
  createModEditorModifyController,
  type ModEditorAnalysisSocketLike,
  type ModEditorModifySocketLike,
} from "./controller.ts";

type AnalyzeStage = "idle" | "scanning" | "streaming" | "done" | "error";
type ModifyStage = "idle" | "running" | "done" | "error";

export function ModEditorFeatureView() {
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
  const [analysisCurrentStage, setAnalysisCurrentStage] = useState<string | null>(null);
  const [analysisStageHistory, setAnalysisStageHistory] = useState<string[]>([]);
  const [analysisErrorMessage, setAnalysisErrorMessage] = useState<string | null>(null);
  const analyzeWsRef = useRef<ModEditorAnalysisSocketLike | null>(null);

  const [modRequest, setModRequest] = useState("");
  const [modifyStage, setModifyStage] = useState<ModifyStage>("idle");
  const [agentLog, setAgentLog] = useState<string[]>([]);
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
    appendAnalysisChunk(chunk) {
      setAnalysisChunks((previous) => [...previous, chunk]);
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
    appendModifyLog(line) {
      setAgentLog((previous) => [...previous, line]);
    },
    completeModify(success) {
      setAgentLog((previous) => [...previous, success ? "✓ 修改完成！" : "✗ 修改失败"]);
      setModifyStage("done");
    },
    failModify(message) {
      setModifyErrorMessage(message);
      setModifyStage("error");
    },
    resetModify() {
      setModifyStage("idle");
      setAgentLog([]);
      setModifyCurrentStage(null);
      setModifyStageHistory([]);
      setModifyErrorMessage(null);
    },
  });

  const analysisText = analysisChunks.join("");
  const isAnalyzing = analyzeStage === "scanning" || analyzeStage === "streaming";
  const isModifying = modifyStage === "running";

  return (
    <div className="max-w-2xl mx-auto space-y-5">
      <div className="rounded-xl border border-amber-300 bg-white shadow-md p-5 space-y-4">
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

      <div className="rounded-xl border border-slate-200 bg-white p-5 space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Search size={15} className="text-slate-400" />
            <h3 className="font-semibold text-slate-700 text-sm">分析 Mod 内容</h3>
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
            className="w-full py-2 rounded-lg border border-amber-400 text-amber-600 font-medium text-sm hover:bg-amber-50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center justify-center gap-2"
          >
            <Search size={14} />
            分析 Mod
          </button>
        )}

        {isAnalyzing && (
          <div className="space-y-3">
            <div className="flex items-center gap-2 py-1">
              <Loader2 size={14} className="text-amber-500 animate-spin" />
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
              <pre className="text-sm text-slate-700 whitespace-pre-wrap font-sans leading-relaxed max-h-80 overflow-y-auto">
                {analysisText}
                {isAnalyzing && <span className="inline-block w-1.5 h-4 bg-amber-400 animate-pulse ml-0.5 align-text-bottom" />}
              </pre>
            )}
          </div>
        )}
      </div>

      <div className="rounded-xl border border-slate-200 bg-white p-5 space-y-4">
        <div className="flex items-center gap-2">
          <Wrench size={15} className="text-slate-400" />
          <h3 className="font-semibold text-slate-700 text-sm">修改 Mod</h3>
        </div>

        <div className="space-y-1">
          <label className="text-xs font-medium text-slate-500">描述要做什么改动</label>
          <textarea
            value={modRequest}
            onChange={(event) => setModRequest(event.target.value)}
            disabled={isModifying}
            rows={4}
            placeholder={"例如：\n把 DarkBlade 的伤害从 8 改成 12，升级后改成 16\n或者：给 FangedGrimoire 增加一个条件，只有血量低于50%时才触发"}
            className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-amber-400 focus:ring-1 focus:ring-amber-100 resize-none disabled:opacity-50"
          />
        </div>

        {modifyStage === "idle" || modifyStage === "done" || modifyStage === "error" ? (
          <div className="flex gap-2">
            <button
              onClick={() => {
                void modifyController.run(projectRoot, modRequest, analysisText);
              }}
              disabled={!projectRoot.trim() || !modRequest.trim()}
              className="flex-1 py-2.5 rounded-lg bg-amber-500 text-white font-bold text-sm hover:bg-amber-600 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
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
              <Loader2 size={14} className="text-amber-500 animate-spin" />
              <span className="text-sm text-slate-400">Code Agent 执行中…</span>
            </div>
            <StageStatus current={modifyCurrentStage} history={modifyStageHistory} />
          </div>
        )}

        {(agentLog.length > 0 || modifyErrorMessage) && (
          <div className="space-y-2">
            {modifyErrorMessage && <pre className="text-xs text-red-600 font-mono whitespace-pre-wrap">{modifyErrorMessage}</pre>}
            {agentLog.length > 0 && <AgentLog lines={agentLog} />}
          </div>
        )}

        {modifyStage === "done" && <BuildDeploy projectRoot={projectRoot} />}
      </div>
    </div>
  );
}
