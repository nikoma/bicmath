//! Statistics module: descriptive statistics, probability distributions,
//! statistical inference, and stratified experiment analysis.

pub mod bayesian;
pub mod causal;
pub mod common;
pub mod continuous_distributions;
pub mod descriptive;
pub mod discrete_distributions;
pub mod distributions;
pub mod inference;
pub mod mathfn;
pub mod regression;
pub mod sequential;
pub mod stratified;
pub mod testing;

use std::sync::Arc;

use bicmath_core::contract::{Function, Module, ModuleDescriptor};
use bicmath_core::number::NumericMode;

/// Build the statistics module with all of its registered functions.
pub fn module() -> Module {
    let mut functions: Vec<Arc<dyn Function>> = Vec::new();
    functions.extend(descriptive::functions());
    functions.extend(distributions::functions());
    functions.extend(discrete_distributions::functions());
    functions.extend(continuous_distributions::functions());
    functions.extend(inference::functions());
    functions.extend(regression::functions());
    functions.extend(testing::functions());
    functions.extend(stratified::functions());
    functions.extend(sequential::functions());
    functions.extend(bayesian::functions());
    functions.extend(causal::functions());
    let descriptor = ModuleDescriptor::new(
        "statistics",
        "Statistics",
        "1.0.0",
        "Descriptive statistics, probability distributions, inference, and stratified \
         experiment analysis.",
    )
    .with_capabilities(vec![
        "exact_descriptive_statistics",
        "probability_distributions",
        "confidence_intervals",
        "sample_size_planning",
        "regression_models",
        "hypothesis_testing",
        "equivalence_testing",
        "stratified_experiments",
        "sequential_inference",
        "confidence_sequences",
        "bayesian_conjugate_updates",
        "causal_estimators",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ])
    .with_source("crates/bicmath-statistics");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bicmath_core::context::ExecContext;
    use bicmath_core::contract::{Args, ExampleExpectation, Function};
    use std::collections::{BTreeMap, BTreeSet};

    fn context_for(function: &dyn Function) -> ExecContext {
        if function.descriptor().modes.contains(&NumericMode::Exact) {
            // Exact-capable functions declare examples in their exact form.
            ExecContext::exact()
        } else {
            ExecContext::scientific()
        }
    }

    #[test]
    fn descriptor_contract_is_complete() {
        let module = module();
        assert_eq!(module.descriptor.id, "statistics");
        assert_eq!(module.descriptor.version, "1.0.0");
        assert!(
            module.functions.len() >= 38,
            "expected all statistics functions"
        );
        let mut ids = BTreeSet::new();
        for function in &module.functions {
            let descriptor = function.descriptor();
            assert!(
                descriptor.id.starts_with("statistics."),
                "bad id {}",
                descriptor.id
            );
            assert_eq!(descriptor.module, "statistics");
            assert_eq!(descriptor.version, "1.0.0");
            assert!(
                descriptor
                    .method_ref
                    .starts_with("docs/methods/statistics.md#"),
                "bad method_ref {}",
                descriptor.method_ref
            );
            assert!(
                !descriptor.examples.is_empty(),
                "function {} has no executable example",
                descriptor.id
            );
            assert!(ids.insert(descriptor.id.clone()), "duplicate id");
        }
    }

    #[test]
    fn every_example_executes_and_matches() {
        for function in module().functions {
            let ctx = context_for(function.as_ref());
            for example in &function.descriptor().examples {
                let mut values = BTreeMap::new();
                for (name, raw) in &example.arguments {
                    let parameter = function
                        .descriptor()
                        .parameter(name)
                        .unwrap_or_else(|| panic!("example parameter {name} is not declared"));
                    parameter
                        .schema
                        .validate(raw, name, &ctx.limits)
                        .unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} has an invalid {name}: {error}",
                                example.title,
                                function.descriptor().id
                            )
                        });
                    values.insert(name.clone(), raw.clone());
                }
                let result = function.invoke(&Args::new(values), &ctx);
                match &example.expected {
                    Some(ExampleExpectation::Value(expected)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} failed: {error}",
                                example.title,
                                function.descriptor().id
                            )
                        });
                        assert_eq!(
                            &outcome.value,
                            expected,
                            "example {:?} of {} produced an unexpected value",
                            example.title,
                            function.descriptor().id
                        );
                    }
                    Some(ExampleExpectation::Error(code)) => {
                        let error = match result {
                            Err(error) => error,
                            Ok(outcome) => panic!(
                                "example {:?} of {} expected {code:?}, produced {:?}",
                                example.title,
                                function.descriptor().id,
                                outcome.value
                            ),
                        };
                        assert_eq!(
                            error.code,
                            *code,
                            "example {:?} of {} produced the wrong error: {}",
                            example.title,
                            function.descriptor().id,
                            error.message
                        );
                    }
                    Some(ExampleExpectation::Contains(text)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} failed: {error}",
                                example.title,
                                function.descriptor().id
                            )
                        });
                        assert!(
                            format!("{:?}", outcome.value).contains(text),
                            "example {:?} of {} does not contain {text:?}",
                            example.title,
                            function.descriptor().id
                        );
                    }
                    None => {
                        // No expectation: the example only has to be invocable.
                        let _ = result;
                    }
                }
            }
        }
    }
}
