//! PostgreSQL client wrapper using tokio-postgres + rustls

use crate::db::{
    schema, ColumnInfo, ConstraintInfo, FunctionInfo, IndexInfo, ResultColumn, ResultRow,
    SchemaInfo, TableInfo, ViewInfo,
};
use anyhow::{Context, Result};
use tokio_postgres::Client;
use tokio_postgres::NoTls;

/// PostgreSQL client with rustls TLS support
pub struct PgClient {
    client: Option<Client>,
}

impl PgClient {
    pub fn new() -> Self {
        Self { client: None }
    }

    /// Connect using a connection string
    pub async fn connect(&mut self, dsn: &str) -> Result<String> {
        // Determine if we need TLS
        let use_tls = dsn.contains("sslmode=require")
            || dsn.contains("sslmode=prefer")
            || dsn.contains("sslmode=verify");

        if use_tls {
            self.connect_tls(dsn).await
        } else {
            self.connect_no_tls(dsn).await
        }
    }

    async fn connect_no_tls(&mut self, dsn: &str) -> Result<String> {
        let (client, connection) = tokio_postgres::connect(dsn, NoTls)
            .await
            .context("Failed to connect to PostgreSQL")?;

        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!("PostgreSQL connection error: {e}");
            }
        });

        // Get server version
        let version: String = client
            .query_one("SELECT version()", &[])
            .await
            .ok()
            .and_then(|row| row.get::<_, Option<String>>(0))
            .unwrap_or_else(|| "PostgreSQL".to_string());

        self.client = Some(client);
        Ok(version)
    }

    async fn connect_tls(&mut self, dsn: &str) -> Result<String> {
        // Build rustls config
        let root_store =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(tls_config);

        let (client, connection) = tokio_postgres::connect(dsn, tls)
            .await
            .context("Failed to connect to PostgreSQL with TLS")?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!("PostgreSQL connection error: {e}");
            }
        });

        let version: String = client
            .query_one("SELECT version()", &[])
            .await
            .ok()
            .and_then(|row| row.get::<_, Option<String>>(0))
            .unwrap_or_else(|| "PostgreSQL".to_string());

        self.client = Some(client);
        Ok(version)
    }

    /// Disconnect
    pub fn disconnect(&mut self) {
        self.client = None;
    }

    fn client(&self) -> Result<&Client> {
        self.client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))
    }

    /// Execute a query and return rows
    pub async fn execute_query(
        &self,
        sql: &str,
    ) -> Result<(Vec<ResultColumn>, Vec<ResultRow>, u64)> {
        let client = self.client()?;

        // Try as a query first (SELECT, etc.)
        let stmt = client
            .prepare(sql)
            .await
            .context("Failed to prepare statement")?;

        let columns: Vec<ResultColumn> = stmt
            .columns()
            .iter()
            .map(|c| ResultColumn {
                name: c.name().to_string(),
            })
            .collect();

        if columns.is_empty() {
            // This is a command (INSERT, UPDATE, DELETE, CREATE, etc.)
            let affected = client
                .execute(&stmt, &[])
                .await
                .context("Failed to execute command")?;
            Ok((vec![], vec![], affected))
        } else {
            let rows = client
                .query(&stmt, &[])
                .await
                .context("Failed to execute query")?;

            let num_cols = columns.len();
            let result_rows: Vec<ResultRow> = rows
                .iter()
                .map(|row| {
                    (0..num_cols)
                        .map(|i| Self::get_column_value(row, i))
                        .collect()
                })
                .collect();

            let count = result_rows.len() as u64;
            Ok((columns, result_rows, count))
        }
    }

    /// Extract a column value as a string, handling common PG types
    fn get_column_value(row: &tokio_postgres::Row, idx: usize) -> Option<String> {
        use tokio_postgres::types::Type;

        let col_type = row.columns()[idx].type_();

        match *col_type {
            Type::BOOL => row.get::<_, Option<bool>>(idx).map(|v| v.to_string()),
            Type::INT2 => row.get::<_, Option<i16>>(idx).map(|v| v.to_string()),
            Type::INT4 => row.get::<_, Option<i32>>(idx).map(|v| v.to_string()),
            Type::INT8 => row.get::<_, Option<i64>>(idx).map(|v| v.to_string()),
            Type::FLOAT4 => row.get::<_, Option<f32>>(idx).map(|v| v.to_string()),
            Type::FLOAT8 => row.get::<_, Option<f64>>(idx).map(|v| v.to_string()),
            Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => {
                row.get::<_, Option<String>>(idx)
            }
            Type::OID => row.get::<_, Option<u32>>(idx).map(|v| v.to_string()),
            _ => {
                // Fallback: try as string
                row.try_get::<_, Option<String>>(idx)
                    .unwrap_or(Some("<binary>".to_string()))
            }
        }
    }

    /// List schemas
    pub async fn list_schemas(&self) -> Result<Vec<SchemaInfo>> {
        let client = self.client()?;
        let rows = client.query(schema::LIST_SCHEMAS, &[]).await?;
        Ok(rows
            .iter()
            .map(|r| SchemaInfo {
                name: r.get(0),
            })
            .collect())
    }

    /// List tables in a schema
    pub async fn list_tables(&self, schema_name: &str) -> Result<Vec<TableInfo>> {
        let client = self.client()?;
        let rows = client.query(schema::LIST_TABLES, &[&schema_name]).await?;
        Ok(rows
            .iter()
            .map(|r| TableInfo {
                name: r.get(0),
                row_estimate: r.get(1),
                total_size: r.get(2),
            })
            .collect())
    }

    /// List views in a schema
    pub async fn list_views(&self, schema_name: &str) -> Result<Vec<ViewInfo>> {
        let client = self.client()?;
        let rows = client.query(schema::LIST_VIEWS, &[&schema_name]).await?;
        Ok(rows
            .iter()
            .map(|r| ViewInfo {
                name: r.get(0),
                definition: r.get::<_, Option<String>>(1).unwrap_or_default(),
            })
            .collect())
    }

    /// List columns for a table
    pub async fn list_columns(
        &self,
        schema_name: &str,
        table_name: &str,
    ) -> Result<Vec<ColumnInfo>> {
        let client = self.client()?;
        let rows = client
            .query(schema::LIST_COLUMNS, &[&schema_name, &table_name])
            .await?;
        Ok(rows
            .iter()
            .map(|r| {
                let nullable: String = r.get(2);
                let pk: String = r.get(4);
                ColumnInfo {
                    name: r.get(0),
                    data_type: r.get(1),
                    is_nullable: nullable == "YES",
                    default_value: r.get(3),
                    is_primary_key: pk == "YES",
                }
            })
            .collect())
    }

    /// List indexes for a table
    pub async fn list_indexes(
        &self,
        schema_name: &str,
        table_name: &str,
    ) -> Result<Vec<IndexInfo>> {
        let client = self.client()?;
        let rows = client
            .query(schema::LIST_INDEXES, &[&schema_name, &table_name])
            .await?;
        Ok(rows
            .iter()
            .map(|r| IndexInfo {
                name: r.get(0),
                is_unique: r.get(1),
                is_primary: r.get(2),
                definition: r.get(3),
            })
            .collect())
    }

    /// List constraints for a table
    pub async fn list_constraints(
        &self,
        schema_name: &str,
        table_name: &str,
    ) -> Result<Vec<ConstraintInfo>> {
        let client = self.client()?;
        let rows = client
            .query(schema::LIST_CONSTRAINTS, &[&schema_name, &table_name])
            .await?;
        Ok(rows
            .iter()
            .map(|r| ConstraintInfo {
                name: r.get(0),
                constraint_type: r.get(1),
                columns: r.get(2),
            })
            .collect())
    }

    /// List functions in a schema
    pub async fn list_functions(&self, schema_name: &str) -> Result<Vec<FunctionInfo>> {
        let client = self.client()?;
        let rows = client
            .query(schema::LIST_FUNCTIONS, &[&schema_name])
            .await?;
        Ok(rows
            .iter()
            .map(|r| FunctionInfo {
                name: r.get(0),
                return_type: r.get(1),
                arguments: r.get(2),
            })
            .collect())
    }
}
