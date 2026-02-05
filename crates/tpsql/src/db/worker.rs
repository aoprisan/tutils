//! Background worker for async PostgreSQL operations

use crate::db::{
    ColumnInfo, ConstraintInfo, FunctionInfo, IndexInfo, PgClient, ResultColumn, ResultRow,
    SchemaInfo, TableInfo, ViewInfo,
};
use std::time::Instant;
use tokio::sync::mpsc;

/// Commands sent from the UI to the database worker
#[derive(Debug)]
pub enum DbCommand {
    /// Connect to a PostgreSQL database
    Connect(String),
    /// Disconnect from the database
    Disconnect,
    /// Execute a SQL query
    ExecuteQuery(String),
    /// Fetch list of schemas
    FetchSchemas,
    /// Fetch tables in a schema
    FetchTables(String),
    /// Fetch views in a schema
    FetchViews(String),
    /// Fetch columns for a table (schema, table)
    FetchColumns(String, String),
    /// Fetch indexes for a table (schema, table)
    FetchIndexes(String, String),
    /// Fetch constraints for a table (schema, table)
    #[allow(dead_code)]
    FetchConstraints(String, String),
    /// Fetch functions in a schema
    FetchFunctions(String),
}

/// Results sent from the database worker back to the UI
#[derive(Debug)]
pub enum DbResult {
    /// Successfully connected (server version)
    Connected(String),
    /// Connection failed
    ConnectionError(String),
    /// Disconnected
    Disconnected,
    /// Query returned rows
    QueryRows {
        columns: Vec<ResultColumn>,
        rows: Vec<ResultRow>,
        execution_time: std::time::Duration,
    },
    /// Query executed (INSERT/UPDATE/DELETE/DDL)
    QueryExecuted {
        affected_rows: u64,
        execution_time: std::time::Duration,
    },
    /// Query failed
    QueryError(String),
    /// Schema list
    Schemas(Vec<SchemaInfo>),
    /// Table list for a schema
    Tables(String, Vec<TableInfo>),
    /// View list for a schema
    Views(String, Vec<ViewInfo>),
    /// Column list for a table
    Columns(String, String, Vec<ColumnInfo>),
    /// Index list for a table
    Indexes(String, String, Vec<IndexInfo>),
    /// Constraints for a table
    Constraints(String, String, Vec<ConstraintInfo>),
    /// Functions for a schema
    Functions(String, Vec<FunctionInfo>),
    /// General error
    Error(String),
}

/// Run the database worker in a background task
pub async fn db_worker(
    mut cmd_rx: mpsc::UnboundedReceiver<DbCommand>,
    result_tx: mpsc::UnboundedSender<DbResult>,
) {
    let mut client = PgClient::new();

    while let Some(cmd) = cmd_rx.recv().await {
        let result = match cmd {
            DbCommand::Connect(dsn) => match client.connect(&dsn).await {
                Ok(version) => DbResult::Connected(version),
                Err(e) => DbResult::ConnectionError(e.to_string()),
            },
            DbCommand::Disconnect => {
                client.disconnect();
                DbResult::Disconnected
            }
            DbCommand::ExecuteQuery(sql) => {
                let start = Instant::now();
                match client.execute_query(&sql).await {
                    Ok((columns, rows, affected)) => {
                        let elapsed = start.elapsed();
                        if columns.is_empty() {
                            DbResult::QueryExecuted {
                                affected_rows: affected,
                                execution_time: elapsed,
                            }
                        } else {
                            DbResult::QueryRows {
                                columns,
                                rows,
                                execution_time: elapsed,
                            }
                        }
                    }
                    Err(e) => DbResult::QueryError(e.to_string()),
                }
            }
            DbCommand::FetchSchemas => match client.list_schemas().await {
                Ok(schemas) => DbResult::Schemas(schemas),
                Err(e) => DbResult::Error(format!("Failed to fetch schemas: {e}")),
            },
            DbCommand::FetchTables(schema) => match client.list_tables(&schema).await {
                Ok(tables) => DbResult::Tables(schema, tables),
                Err(e) => DbResult::Error(format!("Failed to fetch tables: {e}")),
            },
            DbCommand::FetchViews(schema) => match client.list_views(&schema).await {
                Ok(views) => DbResult::Views(schema, views),
                Err(e) => DbResult::Error(format!("Failed to fetch views: {e}")),
            },
            DbCommand::FetchColumns(schema, table) => {
                match client.list_columns(&schema, &table).await {
                    Ok(columns) => DbResult::Columns(schema, table, columns),
                    Err(e) => DbResult::Error(format!("Failed to fetch columns: {e}")),
                }
            }
            DbCommand::FetchIndexes(schema, table) => {
                match client.list_indexes(&schema, &table).await {
                    Ok(indexes) => DbResult::Indexes(schema, table, indexes),
                    Err(e) => DbResult::Error(format!("Failed to fetch indexes: {e}")),
                }
            }
            DbCommand::FetchConstraints(schema, table) => {
                match client.list_constraints(&schema, &table).await {
                    Ok(constraints) => DbResult::Constraints(schema, table, constraints),
                    Err(e) => DbResult::Error(format!("Failed to fetch constraints: {e}")),
                }
            }
            DbCommand::FetchFunctions(schema) => match client.list_functions(&schema).await {
                Ok(functions) => DbResult::Functions(schema, functions),
                Err(e) => DbResult::Error(format!("Failed to fetch functions: {e}")),
            },
        };

        if result_tx.send(result).is_err() {
            break;
        }
    }

    client.disconnect();
}
