import { useState } from "react";
import { api } from "@/services/api";
import type {
  PlanValidationResult,
  ReviewStrictness,
} from "@/services/tauriApi";

const SAMPLE_PLAN = JSON.stringify(
  {
    mod_name: "demo_mod",
    summary: "Sample plan to exercise the validator",
    items: [
      {
        id: "c1",
        type: "card",
        name: "Demo Card",
        description: "A test card",
      },
      {
        id: "c2",
        type: "custom_code",
        name: "Custom Logic",
        depends_on_item_ids: ["c1"],
      },
    ],
  },
  null,
  2,
);

const STRICTNESS_OPTIONS: ReviewStrictness[] = ["efficient", "balanced", "strict"];

export function PlanningCard() {
  const [planText, setPlanText] = useState(SAMPLE_PLAN);
  const [strictness, setStrictness] = useState<ReviewStrictness>("balanced");
  const [result, setResult] = useState<PlanValidationResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  async function handleValidate() {
    setRunning(true);
    setError(null);
    try {
      const plan = JSON.parse(planText);
      const r = (await api.validatePlan(plan, strictness)) as PlanValidationResult;
      setResult(r);
    } catch (e: unknown) {
      setError(String(e));
      setResult(null);
    } finally {
      setRunning(false);
    }
  }

  return (
    <section className="rounded border border-muted/30 p-4">
      <div className="flex items-center justify-between mb-2">
        <h2 className="text-lg font-medium">Planning</h2>
        <div className="flex items-center gap-2">
          <select
            value={strictness}
            onChange={(e) => setStrictness(e.target.value as ReviewStrictness)}
            className="text-sm px-2 py-1 rounded border border-muted/40 bg-transparent"
          >
            {STRICTNESS_OPTIONS.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
          <button
            type="button"
            onClick={handleValidate}
            disabled={running}
            className="text-sm px-3 py-1 rounded border border-muted/40 hover:bg-muted/10 disabled:opacity-50"
          >
            {running ? "Validating…" : "Validate"}
          </button>
        </div>
      </div>

      <textarea
        value={planText}
        onChange={(e) => setPlanText(e.target.value)}
        rows={10}
        spellCheck={false}
        className="w-full font-mono text-xs p-2 rounded border border-muted/30 bg-transparent"
      />

      {error && <p className="text-red-500 text-sm mt-2">Error: {error}</p>}

      {result && (
        <div className="mt-3">
          <p className="text-sm text-muted mb-2">
            Strictness: <span className="font-mono">{result.strictness}</span> · Items:{" "}
            {result.items.length}
          </p>
          <ul className="space-y-1 text-sm">
            {result.items.map((it) => {
              const color =
                it.status === "clear"
                  ? "text-emerald-600"
                  : it.status === "needs_user_input"
                    ? "text-amber-600"
                    : "text-red-600";
              return (
                <li
                  key={`${it.itemId}-${it.status}`}
                  className="border border-muted/20 rounded p-2"
                >
                  <div>
                    <code className="text-xs">{it.itemId || "<no-id>"}</code>
                    <span className={`ml-2 text-xs font-medium ${color}`}>
                      {it.status}
                    </span>
                  </div>
                  {it.issues.length > 0 && (
                    <ul className="text-xs text-red-600 mt-1 ml-3 list-disc">
                      {it.issues.map((iss, i) => (
                        <li key={i}>
                          [{iss.code}] {iss.message}
                          {iss.field && ` (${iss.field})`}
                        </li>
                      ))}
                    </ul>
                  )}
                  {it.missingFields.length > 0 && (
                    <p className="text-xs text-amber-700 mt-1 ml-3">
                      Missing: {it.missingFields.join(", ")}
                    </p>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </section>
  );
}
