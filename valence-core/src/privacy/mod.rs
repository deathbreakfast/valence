//! Schema-driven privacy for entity and field access.
//!
//! [`PrivacyEvaluator`] applies schema/trait policy rules. Built-ins live in
//! [`crate::privacy_policies`] / [`policies`].
//!
//! Empty entity policy lists **deny** non-[`Actor::System`](crate::actor::Actor) viewers.
//! [`crate::query::QueryCore::execute`] drops rows the viewer cannot read.
//! Absent field-level policy means no extra restriction beyond entity checks.
//!
//! Privacy is not a builder port — declare `policies:` on schemas (or `impl` [`PolicyEvaluator`]).
//!
//! ```
//! use valence_core::actor::Actor;
//! use valence_core::privacy::{PrivacyEvaluator, PrivacyPolicy};
//! use valence_core::privacy_policies::common;
//!
//! let policy = PrivacyPolicy {
//!     allow: vec![common::AUTHENTICATED],
//!     ..PrivacyPolicy::default()
//! };
//! let record = serde_json::json!({"id": "1"});
//! assert!(PrivacyEvaluator::evaluate(&policy, &record, &Actor::Anonymous).is_err());
//! ```
mod policy_evaluator;
mod types;

pub use policy_evaluator::PolicyEvaluator;
pub use types::{PrivacyOperation, PrivacyPolicies, PrivacyPolicy, PrivacyRule};

mod evaluator;

pub use evaluator::PrivacyEvaluator;

pub mod policies;
