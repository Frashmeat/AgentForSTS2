import { ModAnalysisSocket, type ModAnalysisEvent } from "../../lib/mod_analysis_ws.ts";
import { WorkflowSocket, type WsEvent } from "../../lib/ws.ts";
import { resolveErrorMessage, resolveWorkflowErrorMessage } from "../../shared/error.ts";

type ModAnalysisSocketEvent = ModAnalysisEvent["event"];
type ModModifySocketEvent = Extract<
  WsEvent["event"],
  "stage_update" | "progress" | "agent_stream" | "done" | "error"
>;

export interface ModEditorAnalysisSocketLike {
  on<T extends ModAnalysisSocketEvent>(
    event: T,
    handler: (data: Extract<ModAnalysisEvent, { event: T }>) => void,
  ): this;
  waitOpen(): Promise<void>;
  send(data: object): void;
  close(): void;
}

export interface ModEditorModifySocketLike {
  on<T extends ModModifySocketEvent>(
    event: T,
    handler: (data: Extract<WsEvent, { event: T }>) => void,
  ): this;
  waitOpen(): Promise<void>;
  send(data: object): void;
  close(): void;
}

export interface ModEditorAnalysisRuntime {
  closeAnalysisSocket(): void;
  setAnalysisSocket(socket: ModEditorAnalysisSocketLike | null): void;
  clearProjectCreationFeedback(): void;
  startAnalysis(): void;
  applyAnalysisStageMessage(message: string): void;
  applyAnalysisScanInfo(files: number): void;
  appendAnalysisChunk(chunk: string): void;
  completeAnalysis(): void;
  failAnalysis(message: string): void;
  resetAnalysis(): void;
}

export interface ModEditorModifyRuntime {
  closeModifySocket(): void;
  setModifySocket(socket: ModEditorModifySocketLike | null): void;
  clearProjectCreationFeedback(): void;
  startModify(): void;
  applyModifyStageMessage(message: string): void;
  appendModifyLog(line: string): void;
  completeModify(success: boolean): void;
  failModify(message: string): void;
  resetModify(): void;
}

interface ModEditorAnalysisDeps {
  createSocket(): ModEditorAnalysisSocketLike;
}

interface ModEditorModifyDeps {
  createSocket(): ModEditorModifySocketLike;
}

export function createModEditorAnalysisController(
  runtime: ModEditorAnalysisRuntime,
  deps: Partial<ModEditorAnalysisDeps> = {},
) {
  const createSocket = deps.createSocket ?? (() => new ModAnalysisSocket());

  function reset() {
    runtime.closeAnalysisSocket();
    runtime.setAnalysisSocket(null);
    runtime.resetAnalysis();
  }

  async function run(projectRoot: string) {
    const normalizedProjectRoot = projectRoot.trim();
    if (!normalizedProjectRoot) {
      return;
    }

    runtime.clearProjectCreationFeedback();
    runtime.closeAnalysisSocket();
    runtime.setAnalysisSocket(null);
    runtime.startAnalysis();

    const socket = createSocket();
    runtime.setAnalysisSocket(socket);
    socket.on("stage_update", (message) => {
      runtime.applyAnalysisStageMessage(message.message);
    });
    socket.on("scan_info", (message) => {
      runtime.applyAnalysisScanInfo(message.files);
    });
    socket.on("stream", (message) => {
      runtime.appendAnalysisChunk(message.chunk);
    });
    socket.on("done", () => {
      runtime.completeAnalysis();
    });
    socket.on("error", (message) => {
      runtime.failAnalysis(resolveWorkflowErrorMessage(message));
    });

    try {
      await socket.waitOpen();
    } catch (error) {
      runtime.setAnalysisSocket(null);
      runtime.failAnalysis(resolveErrorMessage(error));
      return;
    }

    socket.send({ project_root: normalizedProjectRoot });
  }

  return {
    reset,
    run,
  };
}

export function createModEditorModifyController(
  runtime: ModEditorModifyRuntime,
  deps: Partial<ModEditorModifyDeps> = {},
) {
  const createSocket = deps.createSocket ?? (() => new WorkflowSocket());

  function reset() {
    runtime.closeModifySocket();
    runtime.setModifySocket(null);
    runtime.resetModify();
  }

  async function run(projectRoot: string, modRequest: string, analysisText: string) {
    const normalizedProjectRoot = projectRoot.trim();
    const normalizedModRequest = modRequest.trim();
    if (!normalizedProjectRoot || !normalizedModRequest) {
      return;
    }

    runtime.clearProjectCreationFeedback();
    runtime.closeModifySocket();
    runtime.setModifySocket(null);
    runtime.startModify();

    const socket = createSocket();
    runtime.setModifySocket(socket);
    socket.on("stage_update", (message) => {
      runtime.applyModifyStageMessage(message.message);
    });
    socket.on("progress", (message) => {
      runtime.appendModifyLog(message.message);
    });
    socket.on("agent_stream", (message) => {
      runtime.appendModifyLog(message.chunk);
    });
    socket.on("done", (message) => {
      runtime.completeModify(Boolean(message.success));
    });
    socket.on("error", (message) => {
      runtime.failModify(resolveWorkflowErrorMessage(message));
    });

    try {
      await socket.waitOpen();
    } catch (error) {
      runtime.setModifySocket(null);
      runtime.failModify(resolveErrorMessage(error));
      return;
    }

    const normalizedAnalysisText = analysisText.trim();
    const analysisContext = normalizedAnalysisText
      ? `当前 mod 分析概况：\n${normalizedAnalysisText}\n\n`
      : "";

    socket.send({
      action: "start",
      asset_type: "custom_code",
      asset_name: "ModModification",
      description: normalizedModRequest,
      project_root: normalizedProjectRoot,
      implementation_notes:
        analysisContext + "这是对已有 mod 的修改请求，请定位到相关文件进行修改，不要新建不必要的文件。",
    });
  }

  return {
    reset,
    run,
  };
}
