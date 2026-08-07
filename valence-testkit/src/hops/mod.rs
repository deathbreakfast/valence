//! Cross-backend hop layouts and contracts (Cartesian pairs + chains).

mod capability;
mod chain;
mod engines;
mod layout;
mod pair;

pub use capability::{
    hop_adapter_excluded, pair_nested_where_skip, quad_nested_where_skip, triple_nested_where_skip,
    HopSkip,
};

pub use chain::{run_hop_chain_contract, run_hop_quad_contract};
pub use engines::hop_storage_engines;
pub use layout::{
    directed_pairs, hop_quads_representative, hop_triples_representative, HopPair, HopQuad,
    HopTriple,
};
pub use pair::run_hop_pair_contract;
