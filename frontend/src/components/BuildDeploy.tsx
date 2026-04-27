import { useEffect, useRef, useState } from "react";
import { Hammer, CheckCircle2, Loader2, RotateCcw, Settings } from "lucide-react";
import { AgentLog } from "./AgentLog";
import type { StatusNoticeItem } from "./StatusNotice.tsx";
import {
  createBuildDeployController,
  type BuildDeploySocketLike,
} from "./buildDeployController.ts";
import {
  createIdleBuildDeployState,
  describeBuildDeployAction,
  describeBuildDeployCompletionView,
  describeBuildDeployRunningMessage,
  type BuildDeployState,
} from "./buildDeployModel.ts";

interface Props {
  projectRoot: string;
  onOpenSettings?: () => void;
  onStatusNotice?: (notice: Omit<StatusNoticeItem, "id">) => void;
}

export function BuildDeploy({ projectRoot, onOpenSettings, onStatusNotice }: Props) {
  const [state, setState] = useState<BuildDeployState>(() => createIdleBuildDeployState());
  const wsRef = useRef<BuildDeploySocketLike | null>(null);
  const controllerRef = useRef<ReturnType<typeof createBuildDeployController> | null>(null);
  const lastNoticeKeyRef = useRef<string | null>(null);
  const completionView = describeBuildDeployCompletionView(state);

  if (!controllerRef.current) {
    controllerRef.current = createBuildDeployController({
      closeSocket() {
        wsRef.current?.close();
      },
      setSocket(socket) {
        wsRef.current = socket;
      },
      setState(nextState) {
        setState((previous) =>
          typeof nextState === "function" ? nextState(previous) : nextState,
        );
      },
    });
  }
  const controller = controllerRef.current;

  useEffect(() => {
    if (!onStatusNotice) {
      return;
    }

    if (state.stage === "done") {
      const title = completionView?.title ?? state.summary ?? "执行成功";
      const message = completionView?.detail;
      const key = `done:${state.action ?? ""}:${title}:${message ?? ""}`;
      if (lastNoticeKeyRef.current !== key) {
        lastNoticeKeyRef.current = key;
        onStatusNotice({
          title,
          message: message ?? undefined,
          tone: completionView?.tone === "warning" ? "warning" : "success",
        });
      }
      return;
    }

    if (state.stage === "error") {
      const actionText = state.action ? describeBuildDeployAction(state.action) : "执行";
      const message = state.errorMsg ?? "未收到错误详情";
      const key = `error:${state.action ?? ""}:${message}`;
      if (lastNoticeKeyRef.current !== key) {
        lastNoticeKeyRef.current = key;
        onStatusNotice({
          title: `${actionText}失败`,
          message,
          tone: "error",
        });
      }
      return;
    }

    if (state.stage === "running" || state.stage === "idle") {
      lastNoticeKeyRef.current = null;
    }
  }, [completionView, onStatusNotice, state.action, state.errorMsg, state.stage, state.summary]);

  return (
    <div className="space-y-3 pt-3 border-t border-slate-100">
      {state.stage === "idle" && (
        <div className="flex flex-wrap gap-2">
          <button
            onClick={() => void controller.run("deploy", projectRoot)}
            disabled={!projectRoot.trim()}
            className="flex items-center gap-2 py-2 px-4 rounded-lg bg-emerald-500 text-white font-bold text-sm hover:bg-emerald-600 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            <Hammer size={14} />
            构建并部署
          </button>
          <button
            onClick={() => void controller.run("build", projectRoot)}
            disabled={!projectRoot.trim()}
            className="flex items-center gap-2 py-2 px-4 rounded-lg border border-slate-200 bg-white text-slate-700 font-medium text-sm hover:border-amber-300 hover:text-amber-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            仅构建
          </button>
          <button
            onClick={() => void controller.run("package", projectRoot)}
            disabled={!projectRoot.trim()}
            className="flex items-center gap-2 py-2 px-4 rounded-lg border border-slate-200 bg-white text-slate-700 font-medium text-sm hover:border-amber-300 hover:text-amber-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            仅打包
          </button>
        </div>
      )}

      {state.stage === "running" && state.logEntries.length === 0 && state.log.length === 0 && state.action && (
        <div className="flex items-center gap-2 py-1">
          <Loader2 size={14} className="text-emerald-500 animate-spin" />
          <span className="text-sm text-slate-400">{describeBuildDeployRunningMessage(state.action)}</span>
        </div>
      )}

      {(state.log.length > 0 || state.logEntries.length > 0) && (
        <AgentLog lines={state.log} entries={state.logEntries} currentModel={state.currentModel} />
      )}

      {state.stage === "done" && completionView && (
        <div className="space-y-2">
          <div
            className={`flex items-start gap-2 rounded-lg px-3 py-2 ${
              completionView.tone === "warning"
                ? "bg-amber-50 border border-amber-200"
                : "bg-emerald-50 border border-emerald-200"
            }`}
          >
            <CheckCircle2
              size={15}
              className={`shrink-0 mt-0.5 ${
                completionView.tone === "warning" ? "text-amber-500" : "text-emerald-500"
              }`}
            />
            <div className="flex-1">
              <p
                className={`text-sm font-medium ${
                  completionView.tone === "warning" ? "text-amber-700" : "text-emerald-700"
                }`}
              >
                {completionView.title}
              </p>
              {completionView.detail && (
                <p
                  className={`text-xs mt-0.5 break-all ${
                    completionView.tone === "warning" ? "text-amber-600" : "text-emerald-600"
                  } ${completionView.detailMonospace ? "font-mono" : ""}`}
                >
                  {completionView.detail}
                </p>
              )}
            </div>
            {completionView.showOpenSettings && onOpenSettings && (
              <button
                onClick={onOpenSettings}
                className="shrink-0 text-amber-500 hover:text-amber-700 transition-colors"
              >
                <Settings size={14} />
              </button>
            )}
          </div>
          <button
            onClick={() => controller.reset()}
            className="text-xs text-slate-400 hover:text-slate-600 flex items-center gap-1 transition-colors"
          >
            <RotateCcw size={11} /> 重新执行
          </button>
        </div>
      )}

      {state.stage === "error" && (
        <div className="space-y-2">
          <p className="text-xs text-red-600 font-mono">{state.errorMsg}</p>
          <button
            onClick={() => controller.reset()}
            className="text-xs text-slate-400 hover:text-red-500 flex items-center gap-1 transition-colors"
          >
            <RotateCcw size={11} /> 重试
          </button>
        </div>
      )}
    </div>
  );
}
