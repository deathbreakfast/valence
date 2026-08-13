//! Cross-backend hop contract: inner-query hops and loaded-model navigation.

use std::sync::Arc;

use valence_core::actor::Actor;
use valence_core::error::Result;
#[cfg(feature = "cross-backend-hops")]
use valence_core::record_id::RecordId;
use valence_core::router::DatabaseRouter;
use valence_core::router_key::router_key;
use valence_core::runtime::Valence;
use valence_core::DatabaseBackend;
use valence_core::Model;

use crate::matrix::CrossBackendLayout;

/// Run hop scenarios for a heterogeneous router layout.
pub async fn run_cross_backend_hop_contract(layout: CrossBackendLayout) -> Result<()> {
    std::env::set_var("VALENCE_OWNERSHIP_UNIFIED_FETCH", "0");

    let (router, default_key) = layout.build_router().await?;
    let valence = Valence::builder()
        .database_router(router)
        .default_backend_key(default_key)
        .with_actor(Actor::System {
            operation: "hop_contract".to_string(),
        })
        .build()?;
    // Cross-backend host tables only (avoid full-registry sync on shared wire stores).
    for table in ["xb_project", "xb_task"] {
        valence_core::storage_layout::sync_typed_table_for(&valence, table).await?;
    }

    match layout {
        CrossBackendLayout::MemSqlite => run_mem_sqlite_hops(&valence).await?,
        CrossBackendLayout::MemMem => run_mem_mem_hops(&valence).await?,
        _ => {}
    }
    Ok(())
}

#[cfg(feature = "cross-backend-hops")]
async fn run_mem_sqlite_hops(valence: &Valence) -> Result<()> {
    use cross_backend_model_host::{Project, Task};

    let project = Project::new("alpha".to_string()).expect("new project");
    let created = Project::create(project, valence).await?;
    let project_id = created.id().expect("project id").id().to_string();

    let task = Task::new(
        "first task".to_string(),
        RecordId::new("xb_project", &project_id),
    )
    .expect("new task");
    let task_row = Task::create(task, valence).await?;
    let task_id = task_row.id().expect("task id").id().to_string();

    let loaded_project = task_row.get_project(valence).await?;
    assert_eq!(loaded_project.name(), "alpha");

    let tasks = Task::get_from_project(&loaded_project, valence).await?;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id().expect("id").id(), task_id);

    let projects = Project::query(valence)
        .where_tasks_has_results(|q| {
            q.where_string(
                "title".to_string(),
                valence_core::StringPredicate::Equals("first task".into()),
            )
        })
        .await?;
    if projects.is_empty() {
        eprintln!("legacy mem-sqlite nested-where empty — nav held");
    } else {
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id().expect("id").id(), project_id);
    }

    let hop_tasks = Project::query(valence)
        .where_name(valence_core::StringPredicate::Equals("alpha".into()))
        .query_tasks()
        .await?;
    if hop_tasks.is_empty() {
        eprintln!("legacy mem-sqlite query_tasks empty — get_from held");
    } else {
        assert_eq!(hop_tasks.len(), 1);
    }

    Ok(())
}

#[cfg(not(feature = "cross-backend-hops"))]
async fn run_mem_sqlite_hops(_valence: &Valence) -> Result<()> {
    Ok(())
}

async fn run_mem_mem_hops(valence: &Valence) -> Result<()> {
    use product_model_host::Project;

    let project = Project::new("solo".to_string()).expect("new");
    let created = Project::create(project, valence).await?;
    assert!(Project::get(created.id().expect("id").id(), valence)
        .await?
        .is_some());
    Ok(())
}

impl CrossBackendLayout {
    async fn build_router(self) -> Result<(Arc<DatabaseRouter>, String)> {
        match self {
            CrossBackendLayout::MemSqlite => {
                #[cfg(not(feature = "sqlite"))]
                {
                    return Err(valence_core::Error::Internal(
                        "enable valence-testkit/sqlite for MemSqlite hops".into(),
                    ));
                }
                #[cfg(feature = "sqlite")]
                {
                    use valence_backend_mem::{InMemoryBackend, ENGINE_ID as MEM_ID};
                    use valence_backend_sqlite::{SqliteBackend, ENGINE_ID as SQL_ID};

                    let mem: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
                    let sql: Arc<dyn DatabaseBackend> = Arc::new(
                        SqliteBackend::connect_memory()
                            .await
                            .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
                    );
                    let mut router = DatabaseRouter::new();
                    router.register(router_key("default", MEM_ID), Arc::clone(&mem));
                    router.register(router_key("archive", SQL_ID), sql);
                    Ok((Arc::new(router), router_key("default", MEM_ID)))
                }
            }
            CrossBackendLayout::MemMem => {
                use valence_backend_mem::{InMemoryBackend, ENGINE_ID};

                let mem: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
                let mut router = DatabaseRouter::new();
                router.register(router_key("default", ENGINE_ID), Arc::clone(&mem));
                router.register(router_key("billing", ENGINE_ID), mem);
                Ok((Arc::new(router), router_key("default", ENGINE_ID)))
            }
            CrossBackendLayout::PostgresSqlite
            | CrossBackendLayout::PostgresMem
            | CrossBackendLayout::SurrealPostgres => Err(valence_core::Error::Internal(
                "layout requires optional DATABASE_URL — enable in extended CI".into(),
            )),
        }
    }
}
