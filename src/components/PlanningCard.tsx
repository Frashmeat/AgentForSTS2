import { useState } from "react";
import { Badge, Button, Card, Field } from "@/components/ui";
import { ActionableErrorNotice } from "@/components/ActionableErrorNotice";
import { api } from "@/services/api";
import { toActionableFailure } from "@/services/actionableFailure";
import type { ActionableFailure } from "@/services/actionableFailure";
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
  const [error, setError] = useState<ActionableFailure | null>(null);
  const [running, setRunning] = useState(false);

  async function handleValidate() {
    setRunning(true);
    setError(null);
    try {
      const plan = JSON.parse(planText);
      const r = (await api.validatePlan(plan, strictness)) as PlanValidationResult;
      setResult(r);
    } catch (e: unknown) {
      setError(toActionableFailure(e));
      setResult(null);
    } finally {
      setRunning(false);
    }
  }

  return (
    <Card
      eyebrow="planning · validator"
      title="Planning"
      actions={
        <>
          <Field label="strictness">
            <select
              value={strictness}
              onChange={(e) => setStrictness(e.target.value as ReviewStrictness)}
            >
              {STRICTNESS_OPTIONS.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
          </Field>
          <Button size="sm" onClick={handleValidate} disabled={running}>
            {running ? "Validating…" : "Validate"}
          </Button>
        </>
      }
    >
      <Field label="plan JSON">
        <textarea
          value={planText}
          onChange={(e) => setPlanText(e.target.value)}
          rows={10}
          spellCheck={false}
          className="input-mono"
        />
      </Field>

      <ActionableErrorNotice failure={error} className="mt-3" />

      {result && (
        <div className="mt-4">
          <p
            style={{ fontSize: "11.5px", color: "var(--ink-mute)" }}
            className="mb-3"
          >
            strictness <code>{result.strictness}</code> · items {result.items.length}
          </p>
          <ul className="space-y-2">
            {result.items.map((it) => {
              const variant =
                it.status === "clear"
                  ? "ok"
                  : it.status === "needs_user_input"
                    ? "warn"
                    : "error";
              return (
                <li
                  key={`${it.itemId}-${it.status}`}
                  className="p-2.5"
                  style={{
                    background: "var(--paper)",
                    border: "1px solid var(--rule-soft)",
                    borderRadius: "3px",
                  }}
                >
                  <div className="flex items-center gap-2 flex-wrap">
                    <code style={{ fontSize: "11.5px" }}>
                      {it.itemId || "<no-id>"}
                    </code>
                    <Badge variant={variant}>{it.status}</Badge>
                  </div>
                  {it.issues.length > 0 && (
                    <ul
                      className="mt-2 ml-3 list-disc space-y-0.5"
                      style={{ color: "var(--accent-deep)", fontSize: "12px" }}
                    >
                      {it.issues.map((iss, i) => (
                        <li key={i}>
                          [{iss.code}] {iss.message}
                          {iss.field && ` (${iss.field})`}
                        </li>
                      ))}
                    </ul>
                  )}
                  {it.missingFields.length > 0 && (
                    <p
                      className="mt-1 ml-3"
                      style={{ fontSize: "11.5px", color: "var(--gold)" }}
                    >
                      missing: {it.missingFields.join(", ")}
                    </p>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </Card>
  );
}
