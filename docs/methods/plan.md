# Plan module reference

Rule-based method recommendations, experiment checklists, field analysis plans, and interpretation notes.

- Module id: `plan`
- Version: 1.0.0
- Capabilities: method_recommendation, experiment_planning, field_analysis_planning, interpretation_guidance
- Supported modes: exact, auto, scientific
- Functions: 4

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="describe_fields"></a>

## describe_fields

Suggested analyses per field and per field combination, with data-quality caveats.

Takes a list of field records with a name, a type, and an optional observation count and returns fixed rule-based analysis suggestions for each field and for pairs of fields, plus caveats about missingness and multiplicity. The field type is carried under the "type" key because the wire format reserves "kind" for tagged values.

- Module: `plan` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/plan.md#describe_fields

## Parameters

- `fields` — Fields to describe. (array of record with fields name, type, n)

## Output

Field-level analysis plan.

## Examples

### mixed field types

```json
{
  "fields": [
    {
      "n": {
        "kind": "integer",
        "value": "240"
      },
      "name": "region",
      "type": "categorical"
    },
    {
      "n": {
        "kind": "integer",
        "value": "240"
      },
      "name": "channel",
      "type": "categorical"
    },
    {
      "n": {
        "kind": "integer",
        "value": "240"
      },
      "name": "spend",
      "type": "continuous"
    },
    {
      "n": {
        "kind": "integer",
        "value": "240"
      },
      "name": "revenue",
      "type": "money"
    },
    {
      "n": {
        "kind": "integer",
        "value": "240"
      },
      "name": "signup_date",
      "type": "date"
    }
  ]
}
```

Expected: `{"type":"contains","text":"statistics.chi_square_contingency"}`

### negative observation count

```json
{
  "fields": [
    {
      "n": {
        "kind": "integer",
        "value": "-1"
      },
      "name": "age",
      "type": "continuous"
    }
  ]
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="experiment_checklist"></a>

## experiment_checklist

Sample-size guidance, pre-registration checklist, and stopping-rule warnings.

Returns fixed planning guidance for an experiment kind. When planning inputs are supplied they are range-checked and echoed; no sample size is computed here. The checklist points at the registered sample-size and power functions and warns about peeking and sequential testing.

- Module: `plan` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/plan.md#experiment_checklist

## Parameters

- `kind` — Experiment kind. (one of ["two_proportions", "two_means", "stratified", "general"])
- `baseline_rate` (optional) — Baseline event rate in [0, 1], when known. (number)
- `minimum_detectable_effect` (optional) — Smallest effect worth detecting; must be positive. (number)
- `alpha` (optional) — Significance threshold in (0, 1). (number)
- `power` (optional) — Target power in (0, 1). (number)

## Output

Checklist record with sample-size guidance and stopping rules.

## Examples

### two-proportion experiment

```json
{
  "alpha": "0.05",
  "baseline_rate": "0.20",
  "kind": "two_proportions",
  "minimum_detectable_effect": "0.05",
  "power": "0.80"
}
```

Expected: `{"type":"contains","text":"statistics.sample_size_two_proportions"}`

### alpha outside the unit interval

```json
{
  "alpha": "1.5",
  "kind": "two_proportions"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="interpretation_notes"></a>

## interpretation_notes

Generic interpretation caveats for the families of the supplied function ids.

Maps each supplied function id to a method family and returns fixed interpretation caveats for that family: p-values are not effect sizes, IRR may be non-unique, pooled rates do not replace standardization, projections are not observations, and so on. Unknown ids receive a generic note rather than an error.

- Module: `plan` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/plan.md#interpretation_notes

## Parameters

- `functions` — Function ids to interpret. (array of text)

## Output

Interpretation notes grouped by function.

## Examples

### IRR and standardization

```json
{
  "functions": [
    "finance.irr",
    "statistics.stratified_experiment"
  ]
}
```

Expected: `{"type":"contains","text":"multiple real roots"}`

### empty function list

```json
{
  "functions": []
}
```

Expected: `{"type":"error","code":"insufficient_observations"}`


<a id="recommend"></a>

## recommend

Rule-based method recommendations for a task, data shape, and goal.

Pure rule-based planning: the mapping from task, data shape, and goal to recommended function ids is fixed and uses no model, network, or randomness. Recommendations name only functions registered in this build; unavailable methods are called out in the caveats.

- Module: `plan` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/plan.md#recommend

## Parameters

- `task` — Analysis task. (one of ["estimate_mean", "compare_means", "compare_proportions", "estimate_proportion", "correlation", "regression", "experiment_design", "break_even", "cashflow_analysis", "unit_conversion", "root_finding", "optimization", "time_series", "causal_effect"])
- `data_shape` (optional) — Shape of the data; default none. (one of ["one_sample", "two_independent", "two_paired", "many_groups", "bivariate", "multivariate", "none"])
- `goal` (optional) — Analysis goal; default estimate. (one of ["estimate", "test", "predict", "decide"])

## Output

Method recommendation record.

## Examples

### compare two proportions

```json
{
  "data_shape": "two_independent",
  "goal": "test",
  "task": "compare_proportions"
}
```

Expected: `{"type":"contains","text":"statistics.proportions_difference"}`

### plan an experiment

```json
{
  "task": "experiment_design"
}
```

Expected: `{"type":"contains","text":"statistics.sample_size_two_means"}`

### unknown task is rejected

```json
{
  "task": "forecast"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


