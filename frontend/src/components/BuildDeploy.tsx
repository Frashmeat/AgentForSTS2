import { useRef, useState } from "react";
import { Hammer, CheckCircle2, Loader2, RotateCcw, Settings } from "lucide-react";
import { AgentLog } from "./AgentLog";
import { BuildDeploySocket } from "../lib/build_deploy_ws";

type Stage = "idle" | "building" | "done" | "error";

interface Props {
  projectRoot: string;
  onOpenSettings?: () => void;
}

export function BuildDeploy({ projectRoot, onOpenSettings }: Props) {
  const [stage, setStage] = useState<Stage>("idle");
  const [log, setLog] = useState<string[]>([]);
  const [deployedTo, setDeployedTo] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const wsRef = useRef<BuildDeploySocket | null>(null);

  function reset() {
    wsRef.current?.close();
    wsRef.current = null;
    setStage("idle");
    setLog([]);
    setDeployedTo(null);
    setErrorMsg(null);
  }

  async function start() {
    if (!projectRoot.trim()) return;
    setStage("building");
    setLog([]);
    setDeployedTo(null);
    setErrorMsg(null);

    const ws = new BuildDeploySocket();
    wsRef.current = ws;
    ws.on("stream", (msg) => {
      setLog(prev => [...prev, msg.chunk]);
    });
    ws.on("done", (msg) => {
      setDeployedTo(msg.deployed_to ?? null);
      setStage("done");
    });
    ws.on("error", (msg) => {
      setErrorMsg(msg.message);
      setStage("error");
    });

    try {
      await ws.waitOpen();
    } catch (error) {
      setErrorMsg(error instanceof Error ? error.message : String(error));
      setStage("error");
      return;
    }
    ws.send({ project_root: projectRoot.trim() });
  }

  return (
    <div className="space-y-3 pt-3 border-t border-slate-100">
      {stage === "idle" && (
        <button
          onClick={start}
          disabled={!projectRoot.trim()}
          className="flex items-center gap-2 py-2 px-4 rounded-lg bg-emerald-500 text-white font-bold text-sm hover:bg-emerald-600 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
        >
          <Hammer size={14} />
          构建 &amp; 部署
        </button>
      )}

      {stage === "building" && log.length === 0 && (
        <div className="flex items-center gap-2 py-1">
          <Loader2 size={14} className="text-emerald-500 animate-spin" />
          <span className="text-sm text-slate-400">Code Agent 构建中（含 .pck 导出）…</span>
        </div>
      )}

      {log.length > 0 && (
        <AgentLog lines={log} />
      )}

      {stage === "done" && (
        <div className="space-y-2">
          {deployedTo ? (
            <div className="flex items-start gap-2 rounded-lg bg-emerald-50 border border-emerald-200 px-3 py-2">
              <CheckCircle2 size={15} className="text-emerald-500 shrink-0 mt-0.5" />
              <div>
                <p className="text-sm font-medium text-emerald-700">已部署</p>
                <p className="text-xs text-emerald-600 font-mono mt-0.5 break-all">{deployedTo}</p>
              </div>
            </div>
          ) : (
            <div className="flex items-start gap-2 rounded-lg bg-amber-50 border border-amber-200 px-3 py-2">
              <CheckCircle2 size={15} className="text-amber-500 shrink-0 mt-0.5" />
              <div className="flex-1">
                <p className="text-sm font-medium text-amber-700">构建成功，未自动部署</p>
                <p className="text-xs text-amber-600 mt-0.5">在设置中配置 STS2 游戏路径后可自动复制到 Mods 文件夹</p>
              </div>
              {onOpenSettings && (
                <button
                  onClick={onOpenSettings}
                  className="shrink-0 text-amber-500 hover:text-amber-700 transition-colors"
                >
                  <Settings size={14} />
                </button>
              )}
            </div>
          )}
          <button
            onClick={reset}
            className="text-xs text-slate-400 hover:text-slate-600 flex items-center gap-1 transition-colors"
          >
            <RotateCcw size={11} /> 重新构建
          </button>
        </div>
      )}

      {stage === "error" && (
        <div className="space-y-2">
          <p className="text-xs text-red-600 font-mono">{errorMsg}</p>
          <button
            onClick={reset}
            className="text-xs text-slate-400 hover:text-red-500 flex items-center gap-1 transition-colors"
          >
            <RotateCcw size={11} /> 重试
          </button>
        </div>
      )}
    </div>
  );
}
