# Units module reference

Versioned unit registry, dimensional algebra, and exact conversions.

- Module id: `units`
- Version: 1.0.0
- Capabilities: unit_registry, exact_conversions, affine_temperature, dimensional_algebra
- Supported modes: exact, auto, scientific
- Functions: 11

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="add"></a>

## add

Add quantities of identical dimension.

Operands must have identical dimensions; a plain number is dimensionless. Adding absolute temperatures is rejected because the result would not be an absolute temperature.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/units.md#add

## Parameters

- `a` — Left quantity or number. (any value)
- `b` — Right quantity or number. (any value)

## Output

Sum as a quantity, or a plain number when both operands are plain numbers.

## Examples

### sum two durations

```json
{
  "a": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "2"
    },
    "dimension": {
      "time": 1
    }
  },
  "b": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "3"
    },
    "dimension": {
      "time": 1
    }
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"integer","value":"5"},"dimension":{"time":1}}}`


<a id="check"></a>

## check

Evaluate the physical dimensions of a restricted expression without computing values.

Bindings map identifiers to quantities. Literals are dimensionless. Addition, subtraction, remainder, and comparison require equal dimensions; multiplication, division, and integer powers combine dimensions; transcendental functions require dimensionless arguments; sqrt halves even dimension exponents. Returns the resulting dimension, a symbol, and whether it is dimensionless. This catches unit errors before a calculation runs.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/units.md#check

## Parameters

- `expression` — Restricted expression in the bound identifiers, e.g. "distance / time". (text)
- `bindings` — Record mapping identifiers to quantities or dimensionless numbers. (record with fields )

## Output

Record with dimension, symbol, and dimensionless flag.

## Examples

### speed has length over time

```json
{
  "bindings": {
    "distance": {
      "kind": "quantity",
      "value": {
        "kind": "integer",
        "value": "100"
      },
      "dimension": {
        "length": 1
      }
    },
    "time": {
      "kind": "quantity",
      "value": {
        "kind": "integer",
        "value": "10"
      },
      "dimension": {
        "time": 1
      }
    }
  },
  "expression": "distance / time"
}
```

Expected: `{"type":"contains","text":"m*s^-1"}`


<a id="convert"></a>

## convert

Convert a quantity between units of the same dimension.

from and to are registry unit ids. A quantity carries its dimension and is checked against the from unit; a plain number is interpreted in the SI base units of the from unit's dimension. Absolute temperatures apply the affine offset; temperature differences use units.temperature_difference. Results are exact when both factors are exact and approximate when a factor is an approximation such as pi/180.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/units.md#convert

## Parameters

- `value` — Quantity, or plain number in SI base units of the from unit's dimension. (any value)
- `from` — Source unit id, e.g. mile. (text)
- `to` — Target unit id, e.g. kilometer. (text)

## Output

Converted quantity with the target unit's dimension.

## Examples

### one mile in kilometers

```json
{
  "from": "mile",
  "to": "kilometer",
  "value": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "1"
    },
    "dimension": {
      "length": 1
    }
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"decimal","value":"1.609344"},"dimension":{"length":1}}}`

### zero degrees Celsius in kelvin

```json
{
  "from": "degree_celsius",
  "to": "kelvin",
  "value": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "0"
    },
    "dimension": {
      "temperature": 1
    }
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"decimal","value":"273.15"},"dimension":{"temperature":1}}}`

### ambiguous unit identifier

```json
{
  "from": "gallon",
  "to": "liter",
  "value": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "1"
    },
    "dimension": {
      "length": 3
    }
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="describe_unit"></a>

## describe_unit

Describe one unit in the versioned registry, including notes.

Ambiguous identifiers such as gallon, ton, and fluid_ounce are rejected with the qualified alternatives.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/units.md#describe_unit

## Parameters

- `unit` — Registry unit id, symbol, or unambiguous alias, e.g. meter or us_gallon. (text)

## Output

Unit record with notes.

## Examples

### describe the meter

```json
{
  "unit": "meter"
}
```

Expected: `{"type":"value","value":{"affine":false,"aliases":["metre","meters","metres"],"dimension":{"length":{"kind":"integer","value":"1"}},"exact_factor":true,"factor":{"kind":"integer","value":"1"},"id":"meter","name":"meter","notes":"SI base unit of length.","offset":null,"symbol":"m","unit_kind":"length"}}`

### ambiguous identifier

```json
{
  "unit": "gallon"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="divide"></a>

## divide

Divide quantities, subtracting their dimension exponents.

A plain number is treated as dimensionless. Absolute affine temperatures are rejected; only temperature differences may participate in dimensional algebra.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/units.md#divide

## Parameters

- `a` — Dividend quantity or number. (any value)
- `b` — Divisor quantity or number; must not be zero. (any value)

## Output

Quotient as a quantity, or a plain number when both operands are plain numbers.

## Examples

### length from an area and a length

```json
{
  "a": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "6"
    },
    "dimension": {
      "length": 2
    }
  },
  "b": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "2"
    },
    "dimension": {
      "length": 1
    }
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"integer","value":"3"},"dimension":{"length":1}}}`


<a id="is_dimensionless"></a>

## is_dimensionless

True when a value is a plain number or a dimensionless quantity.

Currency is not a physical dimension and is never accepted here.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/units.md#is_dimensionless

## Parameters

- `value` — Quantity or number. (any value)

## Output

True when all dimension exponents are zero.

## Examples

### plain number

```json
{
  "value": {
    "kind": "integer",
    "value": "5"
  }
}
```

Expected: `{"type":"value","value":true}`

### length quantity

```json
{
  "value": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "5"
    },
    "dimension": {
      "length": 1
    }
  }
}
```

Expected: `{"type":"value","value":false}`


<a id="list_units"></a>

## list_units

List the versioned unit registry, optionally filtered by quantity kind.

Returns one record per registered unit with its id, symbol, name, quantity kind, dimension, exact factor to SI base units, affine offset (temperature only), and aliases. Factors are exact rationals except the pi-based angle units, which are flagged with exact_factor=false.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/units.md#list_units

## Parameters

- `quantity_kind` (optional) — Optional kind filter: length, mass, time, current, temperature_absolute, amount, luminous, angle, area, volume, speed, pressure, energy, or power. (text)

## Output

Array of unit records.

## Examples

### list length units

```json
{
  "quantity_kind": "length"
}
```


<a id="multiply"></a>

## multiply

Multiply quantities, adding their dimension exponents.

A plain number is treated as dimensionless. Absolute affine temperatures are rejected; only temperature differences may participate in dimensional algebra.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/units.md#multiply

## Parameters

- `a` — Left quantity or number. (any value)
- `b` — Right quantity or number. (any value)

## Output

Product as a quantity, or a plain number when both operands are plain numbers.

## Examples

### area from two lengths

```json
{
  "a": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "2"
    },
    "dimension": {
      "length": 1
    }
  },
  "b": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "3"
    },
    "dimension": {
      "length": 1
    }
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"integer","value":"6"},"dimension":{"length":2}}}`


<a id="power"></a>

## power

Raise a quantity to an integer power, scaling its dimension exponents.

The exponent must be an integer when the base is a quantity. A plain number accepts any exponent supported by the arithmetic contract. Absolute affine temperatures are rejected.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/units.md#power

## Parameters

- `a` — Base quantity or number. (any value)
- `exponent` — Exponent. (exact number (integer, rational, or decimal))

## Output

Power as a quantity, or a plain number for a plain-number base.

## Examples

### volume from a length

```json
{
  "a": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "2"
    },
    "dimension": {
      "length": 1
    }
  },
  "exponent": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"integer","value":"8"},"dimension":{"length":3}}}`


<a id="subtract"></a>

## subtract

Subtract quantities of identical dimension.

Operands must have identical dimensions. Subtracting absolute temperatures yields a temperature difference.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/units.md#subtract

## Parameters

- `a` — Minuend quantity or number. (any value)
- `b` — Subtrahend quantity or number. (any value)

## Output

Difference as a quantity, or a plain number when both operands are plain numbers.

## Examples

### difference of two lengths

```json
{
  "a": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "5"
    },
    "dimension": {
      "length": 1
    }
  },
  "b": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "2"
    },
    "dimension": {
      "length": 1
    }
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"integer","value":"3"},"dimension":{"length":1}}}`


<a id="temperature_difference"></a>

## temperature_difference

Convert a temperature difference between scales without applying affine offsets.

Only the scale factors are used, so 5 degrees Celsius equals 9 delta degrees Fahrenheit. The result is a temperature-delta quantity, not an absolute temperature.

- Module: `units` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/units.md#temperature_difference

## Parameters

- `value` — Quantity, or plain number in kelvin. (any value)
- `from` — Source temperature unit id. (text)
- `to` — Target temperature unit id. (text)

## Output

Temperature difference in the target scale.

## Examples

### celsius difference in fahrenheit

```json
{
  "from": "degree_celsius",
  "to": "degree_fahrenheit",
  "value": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "5"
    },
    "dimension": {
      "temperature": 1
    }
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"integer","value":"9"},"dimension":{"temperature":1}}}`


