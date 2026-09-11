# Format module reference

Pure text rendering of values as Markdown, LaTeX, CSV, and aligned plain text.

- Module id: `format`
- Version: 1.0.0
- Capabilities: markdown, latex, csv, plain_text, number_rendering
- Supported modes: exact, auto, scientific
- Functions: 5

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="number"></a>

## number

Render one scalar number consistently in the selected style.

The default "plain" style preserves the declared decimal scale, so 0.10 is rendered as "0.10"; integral rationals collapse to integers. "canonical" reproduces the canonical wire string, where rationals always carry an explicit denominator. "scientific" uses exact scientific notation for integers and decimals and shortest-round-trip scientific notation for float64. Rationals are always exact numerator/denominator pairs. The selected style is echoed in the "method" field.

- Module: `format` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/format.md#number
- Units/currency rule: Rendering is textual only: dimension symbols and currency codes are printed, never converted.

## Parameters

- `value` — Number to render. (number)
- `style` (optional) — Output style: canonical, plain (default), or scientific. (one of ["canonical", "plain", "scientific"])

## Output

Record with the rendered text in "text" and the applied style in "method".

## Examples

### plain preserves declared scale

```json
{
  "value": "0.10"
}
```

Expected: `{"type":"value","value":{"method":"plain","text":"0.10"}}`

### scientific notation is exact for decimals

```json
{
  "style": "scientific",
  "value": "0.10"
}
```

Expected: `{"type":"value","value":{"method":"scientific","text":"1.0e-1"}}`

### canonical rational

```json
{
  "style": "canonical",
  "value": {
    "kind": "rational",
    "numerator": "1",
    "denominator": "3"
  }
}
```

Expected: `{"type":"value","value":{"method":"canonical","text":"1/3"}}`


<a id="to_csv"></a>

## to_csv

Render a value as RFC 4180 CSV.

Matrices and arrays of arrays render as rows, arrays of records as a header row plus one row per record, records as key,value pairs, and scalars as a single cell. Fields are quoted when they contain a comma, a double quote, or a line break, and embedded quotes are doubled. Rows are separated by CRLF. A heterogeneous array that cannot be tabulated is rejected.

- Module: `format` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/format.md#to_csv
- Units/currency rule: Rendering is textual only: dimension symbols and currency codes are printed, never converted.

## Parameters

- `value` — Value to render. (any value)

## Output

Record with the rendered CSV in "csv" and the method name in "method".

## Examples

### record array quotes commas and quotes

```json
{
  "value": [
    {
      "name": "Smith, Jane",
      "note": "say \"hi\""
    }
  ]
}
```

Expected: `{"type":"value","value":{"csv":"name,note\r\n\"Smith, Jane\",\"say \"\"hi\"\"\"","method":"csv"}}`

### scalar is a single cell

```json
{
  "value": {
    "kind": "integer",
    "value": "42"
  }
}
```

Expected: `{"type":"value","value":{"csv":"42","method":"csv"}}`


<a id="to_latex"></a>

## to_latex

Render a value as a LaTeX fragment.

Matrices and arrays of records render as tabular environments, arrays of scalars as itemize lists, records as key/value tables, and scalars inside an equation environment. Text is escaped for LaTeX. Values that do not fit a tabular shape render inside a verbatim environment. An optional title is prepended as a section heading.

- Module: `format` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/format.md#to_latex
- Units/currency rule: Rendering is textual only: dimension symbols and currency codes are printed, never converted.

## Parameters

- `value` — Value to render. (any value)
- `title` (optional) — Optional section heading prepended to the rendered fragment. (text)

## Output

Record with the rendered LaTeX in "latex" and the method name in "method".

## Examples

### matrix as a tabular

```json
{
  "value": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  }
}
```

Expected: `{"type":"value","value":{"latex":"\\begin{tabular}{cc}\n1 & 2 \\\\\n3 & 4\n\\end{tabular}","method":"latex"}}`

### special characters are escaped

```json
{
  "value": "50% & 7"
}
```

Expected: `{"type":"value","value":{"latex":"\\begin{equation}\n50\\% \\& 7\n\\end{equation}","method":"latex"}}`


<a id="to_markdown"></a>

## to_markdown

Render a value as a Markdown fragment.

Matrices and arrays of records render as pipe tables (record tables use the sorted union of keys as columns and leave missing cells empty), arrays of scalars as bullet lists, records as key/value tables, and scalars inline. Values that do not fit a tabular shape render as a fenced JSON code block. An optional title is prepended as a level-one heading.

- Module: `format` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/format.md#to_markdown
- Units/currency rule: Rendering is textual only: dimension symbols and currency codes are printed, never converted.

## Parameters

- `value` — Value to render. (any value)
- `title` (optional) — Optional heading prepended to the rendered fragment. (text)

## Output

Record with the rendered Markdown in "markdown" and the method name in "method".

## Examples

### matrix as a table

```json
{
  "value": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  }
}
```

Expected: `{"type":"value","value":{"markdown":"|  |  |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |","method":"markdown"}}`

### money renders its currency

```json
{
  "value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "10.00"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"value","value":{"markdown":"10.00 USD","method":"markdown"}}`

### record array unions keys

```json
{
  "value": [
    {
      "a": {
        "kind": "integer",
        "value": "1"
      },
      "b": {
        "kind": "integer",
        "value": "2"
      }
    },
    {
      "b": {
        "kind": "integer",
        "value": "3"
      },
      "c": {
        "kind": "integer",
        "value": "4"
      }
    }
  ]
}
```

Expected: `{"type":"value","value":{"markdown":"| a | b | c |\n| --- | --- | --- |\n| 1 | 2 |  |\n|  | 3 | 4 |","method":"markdown"}}`


<a id="to_text"></a>

## to_text

Render a value as column-aligned plain text.

Matrices and arrays of records render as column-aligned tables, records as aligned key/value pairs, arrays of scalars as one value per line, and nested values as pretty-printed JSON.

- Module: `format` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/format.md#to_text
- Units/currency rule: Rendering is textual only: dimension symbols and currency codes are printed, never converted.

## Parameters

- `value` — Value to render. (any value)

## Output

Record with the rendered text in "text" and the method name in "method".

## Examples

### aligned matrix

```json
{
  "value": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  }
}
```

Expected: `{"type":"value","value":{"method":"text","text":"1  2\n3  4"}}`

### aligned record table

```json
{
  "value": [
    {
      "a": {
        "kind": "integer",
        "value": "1"
      },
      "bb": {
        "kind": "integer",
        "value": "2"
      }
    },
    {
      "a": {
        "kind": "integer",
        "value": "3"
      },
      "bb": {
        "kind": "integer",
        "value": "4"
      }
    }
  ]
}
```

Expected: `{"type":"value","value":{"method":"text","text":"a  bb\n-  --\n1  2\n3  4"}}`


