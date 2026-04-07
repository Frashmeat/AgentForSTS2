import { BuildDeploySocket, type BuildDeployEvent } from "../lib/build_deploy_ws.ts";
import {
  buildProject,
  packageProject,
  type BuildProjectRequest,
  type BuildProjectResponse,
  type PackageProjectRequest,
  type PackageProjectResponse,
} from "../shared/api/index.ts";
import {
  appendBuildDeployLog,
  applyBuildDeployActionResult,
  createIdleBuildDeployState,
  failBuildDeployAction,
  finalizeBuildProjectResult,
  finalizeDeployResult,
  finalizePackageProjectResult,
  startBuildDeployAction,
  type BuildDeployAction,
  type BuildDeployState,
} from "./buildDeployModel.ts";
import { resolveErrorMessage, resolveWorkflowErrorMessage } from "../shared/error.ts";

export interface BuildDeploySocketLike {
  on<T extends BuildDeployEvent["event"]>(
    event: T,
    handler: (data: Extract<BuildDeployEvent, { event: T }>) => void,
  ): this;
  waitOpen(): Promise<void>;
  send(data: object): void;
  close(): void;
}

export type BuildDeployStateUpdate =
  | BuildDeployState
  | ((previous: BuildDeployState) => BuildDeployState);

export interface BuildDeployControllerRuntime {
  closeSocket(): void;
  setSocket(socket: BuildDeploySocketLike | null): void;
  setState(nextState: BuildDeployStateUpdate): void;
}

interface BuildDeployControllerDeps {
  createSocket(): BuildDeploySocketLike;
  buildProject(request: BuildProjectRequest): Promise<BuildProjectResponse>;
  packageProject(request: PackageProjectRequest): Promise<PackageProjectResponse>;
}

export function createBuildDeployController(
  runtime: BuildDeployControllerRuntime,
  deps: Partial<BuildDeployControllerDeps> = {},
) {
  const createSocket = deps.createSocket ?? (() => new BuildDeploySocket());
  const requestBuildProject = deps.buildProject ?? buildProject;
  const requestPackageProject = deps.packageProject ?? packageProject;

  function reset() {
    runtime.closeSocket();
    runtime.setSocket(null);
    runtime.setState(createIdleBuildDeployState());
  }

  async function run(action: BuildDeployAction, projectRoot: string) {
    const normalizedProjectRoot = projectRoot.trim();
    if (!normalizedProjectRoot) {
      return;
    }

    runtime.closeSocket();
    runtime.setSocket(null);
    runtime.setState(startBuildDeployAction(action));

    if (action === "deploy") {
      const socket = createSocket();
      runtime.setSocket(socket);
      socket.on("stream", (message) => {
        runtime.setState((previous) => appendBuildDeployLog(previous, message.chunk));
      });
      socket.on("done", (message) => {
        runtime.setState((previous) => finalizeDeployResult(previous, message.deployed_to ?? null));
      });
      socket.on("error", (message) => {
        runtime.setState((previous) => failBuildDeployAction(previous, resolveWorkflowErrorMessage(message)));
      });

      try {
        await socket.waitOpen();
      } catch (error) {
        runtime.setSocket(null);
        runtime.setState((previous) => failBuildDeployAction(previous, resolveErrorMessage(error)));
        return;
      }

      socket.send({ project_root: normalizedProjectRoot });
      return;
    }

    try {
      const result =
        action === "build"
          ? finalizeBuildProjectResult(
              await requestBuildProject({ project_root: normalizedProjectRoot }),
            )
          : finalizePackageProjectResult(
              await requestPackageProject({ project_root: normalizedProjectRoot }),
            );
      runtime.setState((previous) => applyBuildDeployActionResult(previous, result));
    } catch (error) {
      runtime.setState((previous) => failBuildDeployAction(previous, resolveErrorMessage(error)));
    }
  }

  return {
    reset,
    run,
  };
}
