//! Pure rendering helpers shared by the format functions.

use std::collections::{BTreeMap, BTreeSet};

use num_bigint::{BigInt, Sign};
use num_traits::Signed;

use bicmath_core::error::EngineError;
use bicmath_core::number::Number;
use bicmath_core::value::{Bound, Value};

/// Scalar rendering style selected by `format.number`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NumberStyle {
    /// Canonical wire string: rationals always carry an explicit denominator.
    Canonical,
    /// Human display: integral rationals collapse to an integer.
    Plain,
    /// Exact scientific notation for integers and decimals, shortest
    /// round-trip scientific notation for float64.
    Scientific,
}

impl NumberStyle {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            NumberStyle::Canonical => "canonical",
            NumberStyle::Plain => "plain",
            NumberStyle::Scientific => "scientific",
        }
    }
}

pub(crate) fn parse_style(text: &str) -> Result<NumberStyle, EngineError> {
    match text {
        "canonical" => Ok(NumberStyle::Canonical),
        "plain" => Ok(NumberStyle::Plain),
        "scientific" => Ok(NumberStyle::Scientific),
        other => Err(EngineError::domain(format!(
            "unknown number style {other:?}; expected canonical, plain, or scientific"
        ))),
    }
}

/// Render a number without losing its declared exactness: decimals keep their
/// declared scale (`0.10` stays `0.10`), rationals stay exact fractions.
pub(crate) fn number_text(number: &Number, style: NumberStyle) -> String {
    match (number, style) {
        (Number::Integer(value), NumberStyle::Scientific) => scientific_digits(value, 0),
        (Number::Decimal(value), NumberStyle::Scientific) => {
            scientific_digits(value.mantissa(), value.scale())
        }
        (Number::Float64(value), NumberStyle::Scientific) => format!("{:e}", value.get()),
        (Number::Rational(value), NumberStyle::Plain) if value.is_integer() => {
            value.numer().to_string()
        }
        (Number::Integer(value), _) => value.to_string(),
        (Number::Rational(value), _) => format!("{}/{}", value.numer(), value.denom()),
        (Number::Decimal(value), _) => value.to_plain_string(),
        (Number::Float64(value), _) => value.to_string(),
    }
}

/// Exact scientific notation for an integer or decimal mantissa/scale pair.
fn scientific_digits(mantissa: &BigInt, scale: u32) -> String {
    if mantissa.sign() == Sign::NoSign {
        return "0e0".to_string();
    }
    let negative = mantissa.sign() == Sign::Minus;
    let digits = mantissa.abs().to_string();
    let exponent = digits.len() as i64 - i64::from(scale) - 1;
    let mut out = String::with_capacity(digits.len() + 8);
    if negative {
        out.push('-');
    }
    out.push(digits.chars().next().unwrap_or('0'));
    if digits.len() > 1 {
        out.push('.');
        out.push_str(&digits[1..]);
    }
    out.push('e');
    out.push_str(&exponent.to_string());
    out
}

/// Inline rendering of a scalar value. Structured values return `None`.
pub(crate) fn scalar_text(value: &Value, style: NumberStyle) -> Option<String> {
    match value {
        Value::Null => Some("null".to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Text(text) => Some(text.clone()),
        Value::Number(number) => Some(number_text(number, style)),
        Value::Quantity { value, dimension } => {
            let number = value.as_number().ok()?;
            let text = number_text(number, style);
            if dimension.is_dimensionless() {
                Some(text)
            } else {
                Some(format!("{text} {}", dimension.symbol()))
            }
        }
        Value::Money { amount, currency } => {
            let number = amount.as_number().ok()?;
            Some(format!("{} {currency}", number_text(number, style)))
        }
        Value::Bound(Bound::Unbounded) => Some("unbounded".to_string()),
        Value::Bound(Bound::Finite(number)) => Some(number_text(number, style)),
        Value::Array(_) | Value::Record(_) | Value::Matrix { .. } => None,
    }
}

/// Cell rendering for tabular output: scalars inline, structured values as
/// compact JSON.
pub(crate) fn cell_text(value: &Value) -> String {
    scalar_text(value, NumberStyle::Plain).unwrap_or_else(|| compact_json(value))
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string())
}

// ---------------------------------------------------------------------------
// Shape classification
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Scalar,
    Matrix,
    Record,
    ArrayOfRecords,
    ArrayOfScalars,
    ArrayOfArrays,
    Nested,
}

fn is_scalar(value: &Value) -> bool {
    scalar_text(value, NumberStyle::Plain).is_some()
}

fn classify(value: &Value) -> Shape {
    match value {
        Value::Array(items) => {
            if items.is_empty() {
                Shape::ArrayOfScalars
            } else if items.iter().all(|item| matches!(item, Value::Record(_))) {
                Shape::ArrayOfRecords
            } else if items.iter().all(is_scalar) {
                Shape::ArrayOfScalars
            } else if items.iter().all(|item| matches!(item, Value::Array(_))) {
                Shape::ArrayOfArrays
            } else {
                Shape::Nested
            }
        }
        Value::Record(_) => Shape::Record,
        Value::Matrix { .. } => Shape::Matrix,
        other if is_scalar(other) => Shape::Scalar,
        _ => Shape::Nested,
    }
}

fn array_items(value: &Value) -> Result<&[Value], EngineError> {
    match value {
        Value::Array(items) => Ok(items),
        other => Err(EngineError::internal(format!(
            "expected an array, found {}",
            other.kind_name()
        ))),
    }
}

fn record_fields(value: &Value) -> Result<&BTreeMap<String, Value>, EngineError> {
    match value {
        Value::Record(fields) => Ok(fields),
        other => Err(EngineError::internal(format!(
            "expected a record, found {}",
            other.kind_name()
        ))),
    }
}

fn matrix_parts(value: &Value) -> Result<(u32, u32, &[Value]), EngineError> {
    match value {
        Value::Matrix { rows, cols, data } => Ok((*rows, *cols, data)),
        other => Err(EngineError::internal(format!(
            "expected a matrix, found {}",
            other.kind_name()
        ))),
    }
}

/// Render every matrix entry, rejecting structured entries: a matrix with
/// non-scalar cells cannot be sensibly tabulated.
fn matrix_rows(value: &Value) -> Result<Vec<Vec<String>>, EngineError> {
    let (rows, cols, data) = matrix_parts(value)?;
    let cols = cols as usize;
    if cols == 0 {
        return Ok(Vec::new());
    }
    if data.len() != rows as usize * cols {
        return Err(EngineError::internal(
            "matrix data length does not match its declared dimensions",
        ));
    }
    let mut out = Vec::with_capacity(rows as usize);
    for (row_index, chunk) in data.chunks(cols).enumerate() {
        let mut row = Vec::with_capacity(cols);
        for (col_index, item) in chunk.iter().enumerate() {
            let index = row_index * cols + col_index;
            row.push(scalar_text(item, NumberStyle::Plain).ok_or_else(|| {
                EngineError::domain(format!(
                    "matrix entry {index} is {}; only scalar entries can be tabulated",
                    item.kind_name()
                ))
            })?);
        }
        out.push(row);
    }
    Ok(out)
}

/// Union of record keys, sorted for stable output.
fn record_columns(items: &[Value]) -> Vec<String> {
    let mut columns = BTreeSet::new();
    for item in items {
        if let Value::Record(fields) = item {
            columns.extend(fields.keys().cloned());
        }
    }
    columns.into_iter().collect()
}

fn record_rows(items: &[Value], columns: &[String]) -> Vec<Vec<String>> {
    items
        .iter()
        .map(|item| match item {
            Value::Record(fields) => columns
                .iter()
                .map(|column| fields.get(column).map(cell_text).unwrap_or_default())
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------

pub(crate) fn markdown_render(value: &Value, title: Option<&str>) -> Result<String, EngineError> {
    let body = match classify(value) {
        Shape::Scalar => scalar_text(value, NumberStyle::Plain).unwrap_or_default(),
        Shape::Matrix => {
            let (_, cols, _) = matrix_parts(value)?;
            let headers = vec![String::new(); cols as usize];
            markdown_table(&headers, &matrix_rows(value)?)
        }
        Shape::ArrayOfRecords => {
            let items = array_items(value)?;
            let columns = record_columns(items);
            markdown_table(&columns, &record_rows(items, &columns))
        }
        Shape::ArrayOfScalars => {
            let items = array_items(value)?;
            items
                .iter()
                .map(|item| format!("- {}", markdown_cell(&cell_text(item))))
                .collect::<Vec<_>>()
                .join("\n")
        }
        Shape::Record => {
            let fields = record_fields(value)?;
            let rows: Vec<Vec<String>> = fields
                .iter()
                .map(|(key, item)| vec![key.clone(), cell_text(item)])
                .collect();
            markdown_table(&["key".to_string(), "value".to_string()], &rows)
        }
        Shape::ArrayOfArrays | Shape::Nested => code_fence(&pretty_json(value), "json"),
    };
    Ok(prepend_markdown_title(title, body))
}

fn markdown_table(headers: &[String], rows: &[Vec<String>]) -> String {
    if headers.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    push_markdown_row(&mut out, headers);
    out.push('\n');
    out.push('|');
    for _ in headers {
        out.push_str(" --- |");
    }
    for row in rows {
        out.push('\n');
        push_markdown_row(&mut out, row);
    }
    out
}

fn push_markdown_row(out: &mut String, cells: &[String]) {
    out.push('|');
    for cell in cells {
        out.push(' ');
        out.push_str(&markdown_cell(cell));
        out.push_str(" |");
    }
}

fn markdown_cell(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '|' => out.push_str("\\|"),
            '\n' | '\r' => out.push(' '),
            _ => out.push(character),
        }
    }
    out
}

fn prepend_markdown_title(title: Option<&str>, body: String) -> String {
    match title {
        Some(title) if !title.is_empty() => {
            let heading: String = title
                .chars()
                .map(|character| match character {
                    '\n' | '\r' => ' ',
                    other => other,
                })
                .collect();
            format!("# {heading}\n\n{body}")
        }
        _ => body,
    }
}

fn code_fence(body: &str, language: &str) -> String {
    let longest_run = body
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest_run.max(2) + 1);
    format!("{fence}{language}\n{body}\n{fence}")
}

// ---------------------------------------------------------------------------
// LaTeX
// ---------------------------------------------------------------------------

pub(crate) fn latex_render(value: &Value, title: Option<&str>) -> Result<String, EngineError> {
    let body = match classify(value) {
        Shape::Scalar => {
            latex_equation(&scalar_text(value, NumberStyle::Plain).unwrap_or_default())
        }
        Shape::Matrix => {
            let (_, cols, _) = matrix_parts(value)?;
            latex_tabular('c', cols as usize, None, &matrix_rows(value)?)
        }
        Shape::ArrayOfRecords => {
            let items = array_items(value)?;
            let columns = record_columns(items);
            latex_tabular(
                'l',
                columns.len(),
                Some(&columns),
                &record_rows(items, &columns),
            )
        }
        Shape::ArrayOfScalars => {
            let items = array_items(value)?;
            let mut out = String::from("\\begin{itemize}\n");
            for item in items {
                out.push_str("\\item ");
                out.push_str(&latex_escape(&cell_text(item)));
                out.push('\n');
            }
            out.push_str("\\end{itemize}");
            out
        }
        Shape::Record => {
            let fields = record_fields(value)?;
            let rows: Vec<Vec<String>> = fields
                .iter()
                .map(|(key, item)| vec![key.clone(), cell_text(item)])
                .collect();
            latex_tabular('l', 2, None, &rows)
        }
        Shape::ArrayOfArrays | Shape::Nested => {
            format!(
                "\\begin{{verbatim}}\n{}\n\\end{{verbatim}}",
                pretty_json(value)
            )
        }
    };
    Ok(prepend_latex_title(title, body))
}

fn latex_equation(text: &str) -> String {
    format!(
        "\\begin{{equation}}\n{}\n\\end{{equation}}",
        latex_escape(text)
    )
}

fn latex_tabular(
    align: char,
    columns: usize,
    header: Option<&[String]>,
    rows: &[Vec<String>],
) -> String {
    let columns = columns.max(1);
    let spec = align.to_string().repeat(columns);
    let mut out = String::new();
    out.push_str(&format!("\\begin{{tabular}}{{{spec}}}\n"));
    if let Some(header) = header {
        push_latex_row(&mut out, header);
        out.push_str(" \\\\\n\\hline\n");
    }
    for (index, row) in rows.iter().enumerate() {
        push_latex_row(&mut out, row);
        if index + 1 < rows.len() {
            out.push_str(" \\\\\n");
        } else {
            out.push('\n');
        }
    }
    out.push_str("\\end{tabular}");
    out
}

fn push_latex_row(out: &mut String, cells: &[String]) {
    let joined = cells
        .iter()
        .map(|cell| latex_escape(cell))
        .collect::<Vec<_>>()
        .join(" & ");
    out.push_str(&joined);
}

fn latex_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '$' => out.push_str("\\$"),
            '&' => out.push_str("\\&"),
            '#' => out.push_str("\\#"),
            '%' => out.push_str("\\%"),
            '_' => out.push_str("\\_"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            _ => out.push(character),
        }
    }
    out
}

fn prepend_latex_title(title: Option<&str>, body: String) -> String {
    match title {
        Some(title) if !title.is_empty() => {
            format!("\\section*{{{}}}\n\n{body}", latex_escape(title))
        }
        _ => body,
    }
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

pub(crate) fn csv_render(value: &Value) -> Result<String, EngineError> {
    match classify(value) {
        Shape::Scalar => Ok(csv_field(
            &scalar_text(value, NumberStyle::Plain).unwrap_or_default(),
        )),
        Shape::Matrix => {
            let rows = matrix_rows(value)?;
            Ok(rows
                .iter()
                .map(|row| csv_row(row))
                .collect::<Vec<_>>()
                .join("\r\n"))
        }
        Shape::ArrayOfRecords => {
            let items = array_items(value)?;
            let columns = record_columns(items);
            let mut lines = Vec::with_capacity(items.len() + 1);
            lines.push(csv_row(&columns));
            for row in record_rows(items, &columns) {
                lines.push(csv_row(&row));
            }
            Ok(lines.join("\r\n"))
        }
        Shape::ArrayOfScalars => {
            let items = array_items(value)?;
            Ok(items
                .iter()
                .map(|item| csv_field(&cell_text(item)))
                .collect::<Vec<_>>()
                .join("\r\n"))
        }
        Shape::ArrayOfArrays => {
            let items = array_items(value)?;
            let lines: Vec<String> = items
                .iter()
                .map(|row| match row {
                    Value::Array(cells) => {
                        csv_row(&cells.iter().map(cell_text).collect::<Vec<_>>())
                    }
                    _ => String::new(),
                })
                .collect();
            Ok(lines.join("\r\n"))
        }
        Shape::Record => {
            let fields = record_fields(value)?;
            let mut lines = vec!["key,value".to_string()];
            for (key, item) in fields {
                lines.push(format!(
                    "{},{}",
                    csv_field(key),
                    csv_field(&cell_text(item))
                ));
            }
            Ok(lines.join("\r\n"))
        }
        Shape::Nested => Err(EngineError::domain(
            "cannot render a heterogeneous value as CSV; expected a matrix, an array of \
             records, an array of arrays, a record, or a scalar",
        )),
    }
}

fn csv_row(cells: &[String]) -> String {
    cells
        .iter()
        .map(|cell| csv_field(cell))
        .collect::<Vec<_>>()
        .join(",")
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

// ---------------------------------------------------------------------------
// Aligned plain text
// ---------------------------------------------------------------------------

pub(crate) fn text_render(value: &Value) -> Result<String, EngineError> {
    match classify(value) {
        Shape::Scalar => Ok(scalar_text(value, NumberStyle::Plain).unwrap_or_default()),
        Shape::Matrix => Ok(aligned_table(None, &matrix_rows(value)?)),
        Shape::ArrayOfRecords => {
            let items = array_items(value)?;
            let columns = record_columns(items);
            Ok(aligned_table(Some(&columns), &record_rows(items, &columns)))
        }
        Shape::ArrayOfScalars => {
            let items = array_items(value)?;
            Ok(items
                .iter()
                .map(|item| table_safe(&cell_text(item)))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        Shape::ArrayOfArrays => {
            let items = array_items(value)?;
            let rows: Vec<Vec<String>> = items
                .iter()
                .map(|row| match row {
                    Value::Array(cells) => cells.iter().map(cell_text).collect(),
                    _ => Vec::new(),
                })
                .collect();
            Ok(aligned_table(None, &rows))
        }
        Shape::Record => {
            let fields = record_fields(value)?;
            let rows: Vec<Vec<String>> = fields
                .iter()
                .map(|(key, item)| vec![key.clone(), cell_text(item)])
                .collect();
            Ok(aligned_table(
                Some(&["key".to_string(), "value".to_string()]),
                &rows,
            ))
        }
        Shape::Nested => Ok(pretty_json(value)),
    }
}

fn aligned_table(header: Option<&[String]>, rows: &[Vec<String>]) -> String {
    let columns = header
        .map(<[String]>::len)
        .unwrap_or_else(|| rows.iter().map(Vec::len).max().unwrap_or(0));
    if columns == 0 {
        return String::new();
    }
    let mut widths = vec![0usize; columns];
    if let Some(header) = header {
        for (index, cell) in header.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index < columns {
                widths[index] = widths[index].max(cell.chars().count());
            }
        }
    }
    let mut lines = Vec::new();
    if let Some(header) = header {
        lines.push(pad_row(header, &widths));
        lines.push(
            widths
                .iter()
                .map(|width| "-".repeat(*width))
                .collect::<Vec<_>>()
                .join("  "),
        );
    }
    for row in rows {
        lines.push(pad_row(row, &widths));
    }
    lines.join("\n")
}

fn pad_row(row: &[String], widths: &[usize]) -> String {
    let last = widths.len().saturating_sub(1);
    widths
        .iter()
        .enumerate()
        .map(|(index, width)| {
            let cell = table_safe(row.get(index).map(String::as_str).unwrap_or(""));
            if index == last {
                cell
            } else {
                let padding = width.saturating_sub(cell.chars().count());
                let mut padded = String::with_capacity(cell.len() + padding);
                padded.push_str(&cell);
                for _ in 0..padding {
                    padded.push(' ');
                }
                padded
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn table_safe(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\n' | '\r' => ' ',
            other => other,
        })
        .collect()
}
