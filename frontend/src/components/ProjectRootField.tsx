import { Loader2 } from "lucide-react";

interface ProjectRootFieldProps {
  label?: string;
  value: string;
  placeholder: string;
  disabled?: boolean;
  showCreateAction?: boolean;
  createActionLabel?: string;
  createBusy: boolean;
  createMessage: string | null;
  createError: string | null;
  onChange: (value: string) => void;
  onCreateProject: () => void;
}

export function ProjectRootField({
  label = "Mod 项目路径",
  value,
  placeholder,
  disabled = false,
  showCreateAction = true,
  createActionLabel = "创建项目",
  createBusy,
  createMessage,
  createError,
  onChange,
  onCreateProject,
}: ProjectRootFieldProps) {
  return (
    <div className="space-y-1">
      <label className="text-xs font-medium text-slate-500">{label}</label>
      <input
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        className="w-full bg-white border border-slate-200 rounded-lg px-3 py-2 text-sm text-slate-800 placeholder:text-slate-300 focus:outline-none focus:border-amber-400 focus:ring-1 focus:ring-amber-100 font-mono disabled:opacity-50"
      />
      {showCreateAction && (
        <button
          onClick={onCreateProject}
          disabled={createBusy || !value.trim() || disabled}
          className="inline-flex items-center gap-1.5 text-xs font-medium text-amber-700 hover:text-amber-800 disabled:text-slate-400 disabled:cursor-not-allowed transition-colors"
        >
          {createBusy ? <Loader2 size={12} className="animate-spin" /> : null}
          {createActionLabel}
        </button>
      )}
      {createMessage && (
        <p className="text-xs text-green-600">{createMessage}</p>
      )}
      {createError && (
        <p className="text-xs text-red-600">{createError}</p>
      )}
    </div>
  );
}
