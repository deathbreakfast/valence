//! Pluggable database backends for Valence.

mod port;
mod used;

#[cfg(feature = "compiler-hybrid")]
mod hybrid_compiler;
#[cfg(feature = "compiler-indradb")]
mod indradb_compiler;
#[cfg(feature = "compiler-mongodb")]
mod mongo_compiler;
#[cfg(feature = "compiler-redis")]
mod redis_compiler;
#[cfg(feature = "compiler-sql")]
mod sql_compiler;
#[cfg(feature = "compiler-surreal")]
mod surreal_compiler;

pub use port::{BackendCapabilities, DatabaseBackend};
pub use used::{
    create_record_used, delete_record_used, get_record_used, merge_record_used, update_record_used,
    upsert_record_used,
};

#[cfg(feature = "compiler-hybrid")]
pub use hybrid_compiler::HybridQueryCompiler;
#[cfg(feature = "compiler-indradb")]
pub use indradb_compiler::IndraQueryCompiler;
#[cfg(feature = "compiler-mongodb")]
pub use mongo_compiler::MongoQueryCompiler;
#[cfg(feature = "compiler-redis")]
pub use redis_compiler::RedisQueryCompiler;
#[cfg(feature = "compiler-sql")]
pub use sql_compiler::SqlQueryCompiler;
#[cfg(feature = "compiler-surreal")]
pub use surreal_compiler::SurrealQueryCompiler;
