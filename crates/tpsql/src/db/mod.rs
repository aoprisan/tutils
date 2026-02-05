//! Database handling for tpsql

pub mod client;
pub mod schema;
pub mod worker;

pub use client::PgClient;
pub use worker::{db_worker, DbCommand, DbResult};

/// Schema information
#[derive(Debug, Clone)]
pub struct SchemaInfo {
    pub name: String,
}

/// Table information
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub row_estimate: i64,
    pub total_size: String,
}

/// View information
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ViewInfo {
    pub name: String,
    pub definition: String,
}

/// Column information
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
}

/// Index information
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IndexInfo {
    pub name: String,
    pub is_unique: bool,
    pub is_primary: bool,
    pub definition: String,
}

/// Constraint information
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ConstraintInfo {
    pub name: String,
    pub constraint_type: String,
    pub columns: String,
}

/// Function information
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub return_type: String,
    pub arguments: String,
}

/// A row of query results
pub type ResultRow = Vec<Option<String>>;

/// Query result column metadata
#[derive(Debug, Clone)]
pub struct ResultColumn {
    pub name: String,
}
