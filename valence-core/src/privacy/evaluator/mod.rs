//! [`PrivacyEvaluator`] — synchronous policy lists and async entity checks.

mod aggregation;
mod defer_to_edge;
mod rules;

/// Privacy evaluation engine
pub struct PrivacyEvaluator;

pub use defer_to_edge::{parent_op_for_defer, DEFER_TO_EDGE_MAX_DEPTH};

#[cfg(test)]
mod tests {
    use super::super::{PolicyEvaluator, PrivacyEvaluator, PrivacyPolicy, PrivacyRule};
    include!("../evaluator_tests_part1.rs");
    include!("../evaluator_tests_part2.rs");
}
