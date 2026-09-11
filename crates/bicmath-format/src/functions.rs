//! Function descriptors, invocations, and registration for the format module.

use std::collections::BTreeMap;
use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, ModuleDescriptor, Outcome,
    ParamDescriptor, SimpleFunction,
};
use bicmath_core::error::EngineError;
use bicmath_core::number::NumericMode;
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;

use crate::render::{
    NumberStyle, csv_render, latex_render, markdown_render, number_text, parse_style, text_render,
};

pub(crate) const MODULE: &str = "format";
pub(crate) const VERSION: &str = "1.0.0";
pub(crate) const UNITS_RULE: &str =
    "Rendering is textual only: dimension symbols and currency codes are printed, never converted.";

fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

fn number_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

fn style_schema() -> ValueSchema {
    ValueSchema::Enum {
        variants: vec![
            "canonical".to_string(),
            "plain".to_string(),
            "scientific".to_string(),
        ],
    }
}

fn rendered_schema(field: &str) -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required(field, ValueSchema::text()),
            FieldSchema::required("method", ValueSchema::text()),
        ],
        allow_extra: false,
    }
}

fn text_record(field: &str, text: String, method: &str) -> Value {
    Value::record([(field, Value::text(text)), ("method", Value::text(method))])
}

fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| (name.to_string(), parse_value(raw.clone())))
        .collect()
}

fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}

// ---------------------------------------------------------------------------
// format.to_markdown
// ---------------------------------------------------------------------------

fn to_markdown_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "format.to_markdown",
        MODULE,
        VERSION,
        "Render as Markdown",
        "Render a value as a Markdown fragment.",
    )
    .with_description(
        "Matrices and arrays of records render as pipe tables (record tables use the \
         sorted union of keys as columns and leave missing cells empty), arrays of \
         scalars as bullet lists, records as key/value tables, and scalars inline. \
         Values that do not fit a tabular shape render as a fenced JSON code block. An \
         optional title is prepended as a level-one heading.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("value", "Value to render.", ValueSchema::Any),
        ParamDescriptor::optional(
            "title",
            "Optional heading prepended to the rendered fragment.",
            ValueSchema::text(),
        ),
    ])
    .with_output(
        rendered_schema("markdown"),
        "Record with the rendered Markdown in \"markdown\" and the method name in \"method\".",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/format.md#to_markdown")
    .with_examples(vec![
        Example::new(
            "matrix as a table",
            example_args(&[(
                "value",
                serde_json::json!({
                    "kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]
                }),
            )]),
        )
        .with_value(text_record(
            "markdown",
            "|  |  |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |".to_string(),
            "markdown",
        )),
        Example::new(
            "money renders its currency",
            example_args(&[(
                "value",
                serde_json::json!({
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "10.00"},
                    "currency": "USD"
                }),
            )]),
        )
        .with_value(text_record("markdown", "10.00 USD".to_string(), "markdown")),
        Example::new(
            "record array unions keys",
            example_args(&[(
                "value",
                serde_json::json!([{"a": 1, "b": 2}, {"b": 3, "c": 4}]),
            )]),
        )
        .with_value(text_record(
            "markdown",
            "| a | b | c |\n| --- | --- | --- |\n| 1 | 2 |  |\n|  | 3 | 4 |".to_string(),
            "markdown",
        )),
    ])
    .with_tags(["formatting", "markdown", "text"])
}

fn invoke_to_markdown(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let value = args.require("value")?;
    let title = args.optional_text("title")?;
    let markdown = markdown_render(value, title)?;
    Ok(Outcome::exact(text_record(
        "markdown", markdown, "markdown",
    )))
}

// ---------------------------------------------------------------------------
// format.to_latex
// ---------------------------------------------------------------------------

fn to_latex_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "format.to_latex",
        MODULE,
        VERSION,
        "Render as LaTeX",
        "Render a value as a LaTeX fragment.",
    )
    .with_description(
        "Matrices and arrays of records render as tabular environments, arrays of scalars \
         as itemize lists, records as key/value tables, and scalars inside an equation \
         environment. Text is escaped for LaTeX. Values that do not fit a tabular shape \
         render inside a verbatim environment. An optional title is prepended as a \
         section heading.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("value", "Value to render.", ValueSchema::Any),
        ParamDescriptor::optional(
            "title",
            "Optional section heading prepended to the rendered fragment.",
            ValueSchema::text(),
        ),
    ])
    .with_output(
        rendered_schema("latex"),
        "Record with the rendered LaTeX in \"latex\" and the method name in \"method\".",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/format.md#to_latex")
    .with_examples(vec![
        Example::new(
            "matrix as a tabular",
            example_args(&[(
                "value",
                serde_json::json!({
                    "kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]
                }),
            )]),
        )
        .with_value(text_record(
            "latex",
            "\\begin{tabular}{cc}\n1 & 2 \\\\\n3 & 4\n\\end{tabular}".to_string(),
            "latex",
        )),
        Example::new(
            "special characters are escaped",
            example_args(&[("value", serde_json::json!("50% & 7"))]),
        )
        .with_value(text_record(
            "latex",
            "\\begin{equation}\n50\\% \\& 7\n\\end{equation}".to_string(),
            "latex",
        )),
    ])
    .with_tags(["formatting", "latex", "text"])
}

fn invoke_to_latex(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let value = args.require("value")?;
    let title = args.optional_text("title")?;
    let latex = latex_render(value, title)?;
    Ok(Outcome::exact(text_record("latex", latex, "latex")))
}

// ---------------------------------------------------------------------------
// format.to_csv
// ---------------------------------------------------------------------------

fn to_csv_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "format.to_csv",
        MODULE,
        VERSION,
        "Render as CSV",
        "Render a value as RFC 4180 CSV.",
    )
    .with_description(
        "Matrices and arrays of arrays render as rows, arrays of records as a header row \
         plus one row per record, records as key,value pairs, and scalars as a single \
         cell. Fields are quoted when they contain a comma, a double quote, or a line \
         break, and embedded quotes are doubled. Rows are separated by CRLF. A \
         heterogeneous array that cannot be tabulated is rejected.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "value",
        "Value to render.",
        ValueSchema::Any,
    )])
    .with_output(
        rendered_schema("csv"),
        "Record with the rendered CSV in \"csv\" and the method name in \"method\".",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/format.md#to_csv")
    .with_examples(vec![
        Example::new(
            "record array quotes commas and quotes",
            example_args(&[(
                "value",
                serde_json::json!([{"name": "Smith, Jane", "note": "say \"hi\""}]),
            )]),
        )
        .with_value(text_record(
            "csv",
            "name,note\r\n\"Smith, Jane\",\"say \"\"hi\"\"\"".to_string(),
            "csv",
        )),
        Example::new(
            "scalar is a single cell",
            example_args(&[("value", serde_json::json!(42))]),
        )
        .with_value(text_record("csv", "42".to_string(), "csv")),
    ])
    .with_tags(["formatting", "csv", "text"])
}

fn invoke_to_csv(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let value = args.require("value")?;
    let csv = csv_render(value)?;
    Ok(Outcome::exact(text_record("csv", csv, "csv")))
}

// ---------------------------------------------------------------------------
// format.to_text
// ---------------------------------------------------------------------------

fn to_text_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "format.to_text",
        MODULE,
        VERSION,
        "Render as aligned text",
        "Render a value as column-aligned plain text.",
    )
    .with_description(
        "Matrices and arrays of records render as column-aligned tables, records as \
         aligned key/value pairs, arrays of scalars as one value per line, and nested \
         values as pretty-printed JSON.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "value",
        "Value to render.",
        ValueSchema::Any,
    )])
    .with_output(
        rendered_schema("text"),
        "Record with the rendered text in \"text\" and the method name in \"method\".",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/format.md#to_text")
    .with_examples(vec![
        Example::new(
            "aligned matrix",
            example_args(&[(
                "value",
                serde_json::json!({
                    "kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]
                }),
            )]),
        )
        .with_value(text_record("text", "1  2\n3  4".to_string(), "text")),
        Example::new(
            "aligned record table",
            example_args(&[(
                "value",
                serde_json::json!([{"a": 1, "bb": 2}, {"a": 3, "bb": 4}]),
            )]),
        )
        .with_value(text_record(
            "text",
            "a  bb\n-  --\n1  2\n3  4".to_string(),
            "text",
        )),
    ])
    .with_tags(["formatting", "plain_text"])
}

fn invoke_to_text(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let value = args.require("value")?;
    let text = text_render(value)?;
    Ok(Outcome::exact(text_record("text", text, "text")))
}

// ---------------------------------------------------------------------------
// format.number
// ---------------------------------------------------------------------------

fn number_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "format.number",
        MODULE,
        VERSION,
        "Format number",
        "Render one scalar number consistently in the selected style.",
    )
    .with_description(
        "The default \"plain\" style preserves the declared decimal scale, so 0.10 is \
         rendered as \"0.10\"; integral rationals collapse to integers. \"canonical\" \
         reproduces the canonical wire string, where rationals always carry an explicit \
         denominator. \"scientific\" uses exact scientific notation for integers and \
         decimals and shortest-round-trip scientific notation for float64. Rationals are \
         always exact numerator/denominator pairs. The selected style is echoed in the \
         \"method\" field.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("value", "Number to render.", number_schema()),
        ParamDescriptor::optional(
            "style",
            "Output style: canonical, plain (default), or scientific.",
            style_schema(),
        ),
    ])
    .with_output(
        rendered_schema("text"),
        "Record with the rendered text in \"text\" and the applied style in \"method\".",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/format.md#number")
    .with_examples(vec![
        Example::new(
            "plain preserves declared scale",
            example_args(&[("value", serde_json::json!("0.10"))]),
        )
        .with_value(text_record("text", "0.10".to_string(), "plain")),
        Example::new(
            "scientific notation is exact for decimals",
            example_args(&[
                ("value", serde_json::json!("0.10")),
                ("style", serde_json::json!("scientific")),
            ]),
        )
        .with_value(text_record("text", "1.0e-1".to_string(), "scientific")),
        Example::new(
            "canonical rational",
            example_args(&[
                (
                    "value",
                    serde_json::json!({
                        "kind": "rational", "numerator": "1", "denominator": "3"
                    }),
                ),
                ("style", serde_json::json!("canonical")),
            ]),
        )
        .with_value(text_record("text", "1/3".to_string(), "canonical")),
    ])
    .with_tags(["formatting", "number", "text"])
}

fn invoke_number(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let number = args.number("value")?;
    let style = match args.optional_text("style")? {
        Some(text) => parse_style(text)?,
        None => NumberStyle::Plain,
    };
    let text = number_text(number, style);
    Ok(Outcome::exact(text_record("text", text, style.as_str())))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Every registered format function, paired with its descriptor.
pub(crate) fn functions() -> Vec<Arc<dyn Function>> {
    vec![
        SimpleFunction::arc(to_markdown_descriptor(), invoke_to_markdown),
        SimpleFunction::arc(to_latex_descriptor(), invoke_to_latex),
        SimpleFunction::arc(to_csv_descriptor(), invoke_to_csv),
        SimpleFunction::arc(to_text_descriptor(), invoke_to_text),
        SimpleFunction::arc(number_descriptor(), invoke_number),
    ]
}

/// The format module descriptor.
pub(crate) fn module_descriptor() -> ModuleDescriptor {
    ModuleDescriptor::new(
        MODULE,
        "Format",
        VERSION,
        "Pure text rendering of values as Markdown, LaTeX, CSV, and aligned plain text.",
    )
    .with_capabilities(vec![
        "markdown",
        "latex",
        "csv",
        "plain_text",
        "number_rendering",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(all_modes())
    .with_source("crates/bicmath-format")
}
