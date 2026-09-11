# Architecture and module-author guide

## Layering

```
                +-------------------------------------------+
                | bicmath   bicmath-mcp   bicmath-wasm  |
                +---------------------+---------------------+
                                      |
                              bicmath-engine
                 (registry, validation, expressions, batches, receipts)
                                      |
      +----------+----------+---------+---------+----------+-----------+
      |          |          |         |         |          |           |
 arithmetic scientific statistics finance    units  linear-algebra    |
      +----------+----------+---------+---------+----------+-----------+
      +----------+----------+---------+---------+----------+-----------+
      |          |          |         |         |          |           |
 interval  optimize    algebra   symbolic  geometry  business  verify  |
      |          |          |         |         |          |      format |
      +----------+----------+---------+---------+----------+--------+----+
                                      |
                              bicmath-core
       (numbers, values, schemas, errors, limits, contracts, envelope)
```

The engine also owns multi-step workflow recipes (`recipes.rs`) that compose
`plan`, calculation, `verify`, and `format` in one request.

Dependency direction is strictly downward. The numerical core contains no
transport, async runtime, filesystem, clock, or network code, which is what
makes the same registry run natively and in the browser. Shared numerical
primitives live in `bicmath-core`; modules never depend on each other.

## The shared numerical contract

`bicmath-core` defines, once:

- `Number`: arbitrary-precision integers, normalized rationals, exact decimals,
  and explicitly identified finite `f64` values, with a documented promotion
  table ([docs/numerics.md](numerics.md)).
- `Value`: the recursive wire model (arrays, records, quantities, money,
  matrices, tagged bounds).
- `EngineError` / `ErrorCode`: stable structured errors.
- `Limits` and `CancellationToken`: resource policy and cooperative
  cancellation.
- `FunctionDescriptor` / `ParamDescriptor` / `ValueSchema`: the authoritative
  schema and metadata used for validation, discovery, and documentation.
- `Outcome` and `ResultEnvelope`: exactness, warnings, assumptions, error
  estimates, traces, fingerprints, and replay receipts.

Every adapter calls the same `Engine`, so there are no parallel arithmetic
implementations.

## Module-author guide

A module is a Rust crate that returns a `Module` value. A function is a plain
function pointer plus a descriptor, wrapped by `SimpleFunction`. Here is a
complete, real extension (also exercised by
`crates/bicmath-engine/tests/extension.rs`):

```rust
use std::sync::Arc;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Module, ModuleDescriptor, Outcome,
    ParamDescriptor, SimpleFunction,
};
use bicmath_core::context::ExecContext;
use bicmath_core::envelope::Exactness;
use bicmath_core::error::EngineError;
use bicmath_core::number::NumericMode;
use bicmath_core::schema::{NumberKind, ValueSchema};
use bicmath_core::value::Value;

fn descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "example.double", "example", "1.0.0", "Double", "Multiply by two.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "value", "Number to double.", ValueSchema::number(NumberKind::Any),
    )])
    .with_output(ValueSchema::number(NumberKind::Any), "Twice the input.")
    .with_modes(vec![NumericMode::Exact, NumericMode::Auto, NumericMode::Scientific])
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/architecture.md#module-author-guide")
    .with_examples(vec![/* at least one executable example */])
}

fn invoke(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = args.number("value")?;
    let result = value.mul(&bicmath_core::number::Number::integer(2), &ctx.numeric, &ctx.limits)?;
    Ok(Outcome::new(Value::Number(result.value), Exactness::Exact))
}

pub fn module() -> Module {
    let functions: Vec<Arc<dyn bicmath_core::contract::Function>> =
        vec![SimpleFunction::arc(descriptor(), invoke)];
    Module::new(
        ModuleDescriptor::new("example", "Example", "1.0.0", "An extension module."),
        functions,
    )
}
```

A downstream application statically registers it:

```rust
use bicmath_engine::{Engine, EngineConfig, RegistryBuilder};

let registry = RegistryBuilder::new()
    .add_module(my_crate::module())
    .build()
    .expect("modules validate");
let engine = Engine::from_registry(registry, EngineConfig::default());
```

That is the whole integration: the expression parser, MCP tool generation,
validation, discovery, receipts, and CLI commands pick the module up
automatically. Registration validates duplicate ids, unresolved module
dependencies, mode consistency, and example argument coercion at startup.
`Engine::validate_examples` executes every documented example, and CI fails if
any example does not run.

### Checklist for a new function

1. Stable qualified id `module.name` and a version.
2. Typed parameter schemas with descriptions, required/optional flags, and
   domain constraints.
3. Output schema and description.
4. Supported numeric modes, purity, determinism, cost class.
5. Method reference, units/currency rule, and at least one executable example
   with an expected value or error.
6. Tests with hand-checkable or published reference values; domain errors for
   invalid inputs; no panics or `unwrap` on user input.
7. No filesystem, network, clock, or randomness use. Return structured
   `EngineError` with the right `ErrorCode`.
8. Keep `ctx.check()` inside loops so cancellation and deadlines work.

## Adapters

- `bicmath-engine` is synchronous and WASM-safe.
- `bicmath-mcp` runs CPU work on bounded blocking workers, bridges MCP
  cancellation to the engine token, and generates tool schemas from the same
  descriptors.
- `bicmath` is the native executable; JSON is the default output.
- `bicmath-wasm` exposes the engine to JavaScript with string-safe exact
  payloads.

## Runtime module disablement

Compiled modules can be disabled in configuration (`disabled_modules`).
Disabled modules disappear from `list_modules`/`list_functions`, and calls
through `calculate`, `evaluate`, and `batch` return `disabled_module`.
