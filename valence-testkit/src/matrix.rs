//! Matrix dimensions for Valence e2e and bench.

use std::fmt;

use crate::bootstrap::WireBackendOptions;

/// Storage adapter under test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageAdapter {
    Mem,
    Sqlite,
    Postgres,
    MongoDb,
    IndraDb,
    Redis,
    SurrealMem,
    SurrealRocksdb,
    AcmeStub,
}

impl StorageAdapter {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Mem => "mem",
            Self::Sqlite => "sqlite",
            Self::Postgres => "postgres",
            Self::MongoDb => "mongodb",
            Self::IndraDb => "indradb",
            Self::Redis => "redis",
            Self::SurrealMem => "surreal-mem",
            Self::SurrealRocksdb => "surreal-rocksdb",
            Self::AcmeStub => "acme-stub",
        }
    }

    pub fn parse_cli(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "mem" => Some(Self::Mem),
            "sqlite" => Some(Self::Sqlite),
            "postgres" | "postgresql" => Some(Self::Postgres),
            "mongodb" | "mongo" => Some(Self::MongoDb),
            "indradb" | "indra" => Some(Self::IndraDb),
            "redis" => Some(Self::Redis),
            "surreal-mem" | "surreal_mem" => Some(Self::SurrealMem),
            "surreal-rocksdb" | "surreal_rocksdb" | "rocksdb" => Some(Self::SurrealRocksdb),
            "acme-stub" | "acme_stub" | "acme" => Some(Self::AcmeStub),
            _ => None,
        }
    }

    /// Whether generated model CRUD scenarios apply (concrete document/graph backends).
    pub fn supports_model_runtime(self) -> bool {
        !matches!(self, Self::AcmeStub)
    }

    /// Whether admin runtime contract applies (needs a registered query compiler).
    pub fn supports_admin_runtime(self) -> bool {
        !matches!(self, Self::AcmeStub)
    }

    /// Whether Surreal-only inventory bootstrap scenarios apply.
    pub fn supports_surreal_inventory(self) -> bool {
        matches!(self, Self::SurrealMem | Self::SurrealRocksdb)
    }
}

/// Cross-backend router layouts for hop contract tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CrossBackendLayout {
    /// Project on mem, task on sqlite.
    MemSqlite,
    /// Two logical mem backends (default + billing).
    MemMem,
    /// Project on postgres, task on sqlite (requires DATABASE_URL).
    PostgresSqlite,
    /// Project on postgres, task on mem (requires DATABASE_URL).
    PostgresMem,
    /// Project on surreal, task on postgres (requires DATABASE_URL).
    SurrealPostgres,
}

impl CrossBackendLayout {
    pub fn slug(self) -> &'static str {
        match self {
            Self::MemSqlite => "mem-sqlite",
            Self::MemMem => "mem-mem",
            Self::PostgresSqlite => "postgres-sqlite",
            Self::PostgresMem => "postgres-mem",
            Self::SurrealPostgres => "surreal-postgres",
        }
    }
}

/// All storage adapters enabled in the current build (feature-gated).
pub fn all_storage_adapters() -> Vec<StorageAdapter> {
    let mut out = vec![
        StorageAdapter::Mem,
        StorageAdapter::Sqlite,
        StorageAdapter::MongoDb,
        StorageAdapter::IndraDb,
        StorageAdapter::Redis,
    ];
    if cfg!(feature = "postgres") {
        out.push(StorageAdapter::Postgres);
    }
    if cfg!(feature = "surreal-mem") {
        out.push(StorageAdapter::SurrealMem);
    }
    if cfg!(feature = "surreal-rocksdb") {
        out.push(StorageAdapter::SurrealRocksdb);
    }
    if cfg!(feature = "acme-stub") {
        out.push(StorageAdapter::AcmeStub);
    }
    out
}

/// Telemetry sink selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TelemetryAdapter {
    #[default]
    Off,
    Console,
    Recording,
}

impl TelemetryAdapter {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Console => "console",
            Self::Recording => "recording",
        }
    }

    pub fn parse_cli(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" | "0" | "false" => Some(Self::Off),
            "console" => Some(Self::Console),
            "recording" | "record" => Some(Self::Recording),
            _ => None,
        }
    }
}

/// Deployment topology (embedded vs remote).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Topology {
    #[default]
    Embedded,
    RemoteStub,
}

impl Topology {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::RemoteStub => "remote-stub",
        }
    }

    pub fn parse_cli(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "embedded" => Some(Self::Embedded),
            "remote-stub" | "remote_stub" | "remote" => Some(Self::RemoteStub),
            _ => None,
        }
    }
}

/// One matrix row: storage × telemetry × topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatrixSpec {
    pub storage: StorageAdapter,
    pub telemetry: TelemetryAdapter,
    pub topology: Topology,
}

impl Default for MatrixSpec {
    fn default() -> Self {
        Self {
            storage: StorageAdapter::Mem,
            telemetry: TelemetryAdapter::Off,
            topology: Topology::Embedded,
        }
    }
}

impl MatrixSpec {
    pub fn ci_mem_embedded() -> Self {
        Self {
            storage: StorageAdapter::Mem,
            telemetry: TelemetryAdapter::Off,
            topology: Topology::Embedded,
        }
    }

    pub fn ci_surreal_mem_embedded() -> Self {
        Self {
            storage: StorageAdapter::SurrealMem,
            telemetry: TelemetryAdapter::Off,
            topology: Topology::Embedded,
        }
    }

    pub fn ci_acme_stub_embedded() -> Self {
        Self {
            storage: StorageAdapter::AcmeStub,
            telemetry: TelemetryAdapter::Off,
            topology: Topology::Embedded,
        }
    }

    pub fn slug(&self) -> String {
        format!(
            "{}_{}_{}",
            self.storage.slug(),
            self.telemetry.slug(),
            self.topology.slug()
        )
    }
}

impl fmt::Display for MatrixSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slug())
    }
}

/// Whether a wire backend builder can resolve for this storage adapter.
pub fn wire_backend_configured(storage: StorageAdapter, wire: Option<&WireBackendOptions>) -> bool {
    match storage {
        StorageAdapter::Postgres => {
            #[cfg(feature = "postgres")]
            {
                let builder = wire
                    .and_then(|o| o.postgres.clone())
                    .unwrap_or_else(valence_backend_postgres::PostgresBackendBuilder::new);
                builder.from_env_defaults().resolve().is_ok()
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = wire;
                false
            }
        }
        StorageAdapter::MongoDb => {
            #[cfg(feature = "mongodb")]
            {
                let builder = wire
                    .and_then(|o| o.mongodb.clone())
                    .unwrap_or_else(valence_backend_mongodb::MongoBackendBuilder::new);
                builder.from_env_defaults().resolve().is_ok()
            }
            #[cfg(not(feature = "mongodb"))]
            {
                let _ = wire;
                false
            }
        }
        StorageAdapter::Redis => {
            #[cfg(feature = "redis")]
            {
                if let Some(fleet) = wire.and_then(|o| o.redis_fleet.clone()) {
                    return fleet.from_env_defaults().resolve().is_ok();
                }
                let builder = wire
                    .and_then(|o| o.redis.clone())
                    .unwrap_or_else(valence_backend_redis::RedisBackendBuilder::new);
                builder.from_env_defaults().resolve().is_ok()
            }
            #[cfg(not(feature = "redis"))]
            {
                let _ = wire;
                false
            }
        }
        _ => true,
    }
}

/// Whether this matrix row can run in the current build/environment.
pub fn extended_store_available(storage: StorageAdapter) -> bool {
    extended_store_available_with_wire(storage, None)
}

/// Whether this matrix row can run with optional wire builder options.
pub fn extended_store_available_with_wire(
    storage: StorageAdapter,
    wire: Option<&WireBackendOptions>,
) -> bool {
    match storage {
        StorageAdapter::Mem
        | StorageAdapter::Sqlite
        | StorageAdapter::IndraDb
        | StorageAdapter::SurrealMem
        | StorageAdapter::AcmeStub => true,
        StorageAdapter::MongoDb => {
            cfg!(feature = "mongodb") && wire_backend_configured(storage, wire)
        }
        StorageAdapter::Redis => cfg!(feature = "redis") && wire_backend_configured(storage, wire),
        StorageAdapter::Postgres => {
            cfg!(feature = "postgres") && wire_backend_configured(storage, wire)
        }
        StorageAdapter::SurrealRocksdb => {
            cfg!(feature = "surreal-rocksdb")
                && std::env::var("VALENCE_BENCH_ROCKSDB")
                    .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        }
    }
}

pub fn extended_store_skip_reason(storage: StorageAdapter) -> Option<String> {
    extended_store_skip_reason_with_wire(storage, None)
}

pub fn extended_store_skip_reason_with_wire(
    storage: StorageAdapter,
    wire: Option<&WireBackendOptions>,
) -> Option<String> {
    if extended_store_available_with_wire(storage, wire) {
        return None;
    }
    match storage {
        StorageAdapter::Postgres => {
            if !cfg!(feature = "postgres") {
                Some("enable valence-testkit/postgres".into())
            } else {
                Some(
                    "configure PostgresBackendBuilder.url() or DATABASE_URL via from_env_defaults"
                        .into(),
                )
            }
        }
        StorageAdapter::MongoDb => {
            if !cfg!(feature = "mongodb") {
                Some("enable valence-testkit/mongodb".into())
            } else {
                Some("configure MongoBackendBuilder.uri() or VALENCE_MONGODB_URI via from_env_defaults".into())
            }
        }
        StorageAdapter::Redis => {
            if !cfg!(feature = "redis") {
                Some("enable valence-testkit/redis".into())
            } else {
                Some("configure RedisBackendBuilder.url() or VALENCE_REDIS_URL via from_env_defaults".into())
            }
        }
        StorageAdapter::SurrealRocksdb => {
            Some("set VALENCE_BENCH_ROCKSDB=1 and enable valence-testkit/surreal-rocksdb".into())
        }
        _ => Some(format!("storage adapter {} unavailable", storage.slug())),
    }
}

pub fn topology_available(topology: Topology) -> bool {
    match topology {
        Topology::Embedded => true,
        Topology::RemoteStub => false,
    }
}

pub fn topology_skip_reason(topology: Topology) -> Option<String> {
    if topology_available(topology) {
        return None;
    }
    Some("remote topology owned by host wiring — stub only".into())
}
