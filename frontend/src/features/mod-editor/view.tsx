import { useEffect, useRef, useState } from "react";
import { Loader2, RotateCcw, Search, Wrench } from "lucide-react";

import { AgentLog } from "../../components/AgentLog";
import { BuildDeploy } from "../../components/BuildDeploy";
import { StageStatus } from "../../components/StageStatus";
import { ModAnalysisSocket } from "../../lib/mod_analysis_ws";
import { WorkflowSocket } from "../../lib/ws";
import { loadAppConfig } from "../../shared/api/index.ts";

type AnalyzeStage = "idle" | "scanning" | "streaming" | "done" | "error";
type ModifyStage = "idle" | "running" | "done" | "error";

export function ModEditorFeatureView() {
  const [projectRoot, setProjectRoot] = useState("");

  useEffect(() => {
    loadAppConfig()
      .then((config) => {
        if (config?.default_project_root) {
          setProjectRoot(String(config.default_project_root));
        }
      })
      .catch(() => {});
  }, []);

  const [analyzeStage, setAnalyzeStage] = useState<AnalyzeStage>("idle");
  const [scanFiles, setScanFiles] = useState<number | null>(null);
  const [analysisChunks, setAnalysisChunks] = useState<string[]>([]);
  const [analysisCurrentStage, setAnalysisCurrentStage] = useState<string | null>(null);
  const [analysisStageHistory, setAnalysisStageHistory] = useState<string[]>([]);
  const [analysisError, setAnalysisError] = useState<string | null>(null);
  const analyzeWsRef = useRef<ModAnalysisSocket | null>(null);

  const [modRequest, setModRequest] = useState("");
  const [modifyStage, setModifyStage] = useState<ModifyStage>("idle");
  const [agentLog, setAgentLog] = useState<string[]>([]);
  const [modifyCurrentStage, setModifyCurrentStage] = useState<string | null>(null);
  const [modifyStageHistory, setModifyStageHistory] = useState<string[]>([]);
  const [modifyError, setModifyError] = useState<string | null>(null);
  const modifyWsRef = useRef<WorkflowSocket | null>(null);

  async function startAnalysis() {
    if (!projectRoot.trim()) {
      return;
    }
    setAnalyzeStage("scanning");
    setScanFiles(null);
    setAnalysisChunks([]);
    setAnalysisCurrentStage(null);
    setAnalysisStageHistory([]);
    setAnalysisError(null);

    const ws = new ModAnalysisSocket();
    analyzeWsRef.current = ws;
    ws.on("stage_update", (message) => {
      setAnalysisCurrentStage(message.message);
      setAnalysisStageHistory((previous) =>
        previous[previous.length - 1] === message.message ? previous : [...previous, message.message]
      );
    });
    ws.on("scan_info", (message) => {
      setScanFiles(message.files);
      setAnalyzeStage("streaming");
    });
    ws.on("stream", (message) => {
      setAnalysisChunks((previous) => [...previous, message.chunk]);
    });
    ws.on("done", () => {
      setAnalyzeStage("done");
    });
    ws.on("error", (message) => {
      setAnalysisError(message.message);
      setAnalyzeStage("error");
    });

    try {
      await ws.waitOpen();
    } catch (error) {
      setAnalysisError(error instanceof Error ? error.message : String(error));
      setAnalyzeStage("error");
      return;
    }
    ws.send({ project_root: projectRoot.trim() });
  }

  function resetAnalysis() {
    analyzeWsRef.current?.close();
    analyzeWsRef.current = null;
    setAnalyzeStage("idle");
    setScanFiles(null);
    setAnalysisChunks([]);
    setAnalysisCurrentStage(null);
    setAnalysisStageHistory([]);
    setAnalysisError(null);
  }

  async function startModify() {
    if (!projectRoot.trim() || !modRequest.trim()) {
      return;
    }
    setModifyStage("running");
    setAgentLog([]);
    setModifyCurrentStage(null);
    setModifyStageHistory([]);
    setModifyError(null);

    const ws = new WorkflowSocket();
    modifyWsRef.current = ws;

    ws.on("stage_update", (data: any) => {
      setModifyCurrentStage(data.message);
      setModifyStageHistory((previous) =>
        previous[previous.length - 1] === data.message ? previous : [...previous, data.message]
      );
    });
    ws.on("progress", (data: any) => setAgentLog((previous) => [...previous, data.message]));
    ws.on("agent_stream", (data: any) => setAgentLog((previous) => [...previous, data.chunk]));
    ws.on("done", (data: any) => {
      setAgentLog((previous) => [...previous, data.success ? "✓ 修改完成！" : "✗ 修改失败"]);
      setModifyStage("done");
    });
    ws.on("error", (data: any) => {
      setModifyError(data.message);
      setModifyStage("error");
    });

    try {
      await ws.waitOpen();
    } catch (error) {
      setModifyError(error instanceof Error ? error.message : String(error));
      setModifyStage("error");
      return;
    }

    const analysisContext =
      analysisChunks.length > 0 ? `当前 mod 分析概况：\n${analysisChunks.join("")}\n\n` : "";

    ws.send({
      action: "start",
      asset_type: "custom_code",
      asset_name: "ModModification",
      description: modRequest.trim(),
      project_root: projectRoot.trim(),
      implementation_notes: analysisContext + "这是对已有 mod 的修改请求，请定位到相关文件进行修改，不要新建不必要的文件。",
    });
  }

  function resetModify() {
    modifyWsRef.current?.close();
    modifyWsRef.current = null;
    setModifyStage("idle");
    setAgentLog([]);
    setModifyCurrentStage(null);
    setModifyStageHistory([]);
    setModifyError(null);
  }

  const analysisText = analysisChunks.join("");
  const isAnalyzing = analyzeStage === "scanning" || analyzeStage === "streaming";
  const isModifying = modifyStage === "running";

  return (
    <div className="max-w-2xl mx-auto space-y-5">
      <div className="rounded-xl border border-amber-300 bg-white shadow-md p-5 space-y-4">
        <h2 className="font-semibold text-slate-800">Mod 项目路径</h2>
        <input
          value={projectRoot}
          onChange={(event) => setProjectRoot(event.target.value)}
          placeholder="E:/STS2mod/testscenario/MyMod"
          className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-amber-400 focus:ring-1 focus:ring-amber-100 font-mono"
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
              onClick={resetAnalysis}
              className="text-xs text-slate-400 hover:text-slate-600 flex items-center gap-1 transition-colors"
            >
              <RotateCcw size={11} /> 重新分析
            </button>
          )}
        </div>

        {analyzeStage === "idle" && (
          <button
            onClick={startAnalysis}
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

        {(analysisText || analysisError) && (
          <div className="space-y-2">
            {analysisError ? (
              <pre className="text-xs text-red-600 font-mono whitespace-pre-wrap">{analysisError}</pre>
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
              onClick={startModify}
              disabled={!projectRoot.trim() || !modRequest.trim()}
              className="flex-1 py-2.5 rounded-lg bg-amber-500 text-white font-bold text-sm hover:bg-amber-600 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              执行修改
            </button>
            {(modifyStage === "done" || modifyStage === "error") && (
              <button
                onClick={resetModify}
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

        {(agentLog.length > 0 || modifyError) && (
          <div className="space-y-2">
            {modifyError && <pre className="text-xs text-red-600 font-mono whitespace-pre-wrap">{modifyError}</pre>}
            {agentLog.length > 0 && <AgentLog lines={agentLog} />}
          </div>
        )}

        {modifyStage === "done" && <BuildDeploy projectRoot={projectRoot} />}
      </div>
    </div>
  );
}
