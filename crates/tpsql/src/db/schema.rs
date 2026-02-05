//! SQL constants for schema introspection

/// List all non-system schemas
pub const LIST_SCHEMAS: &str = "\
SELECT schema_name
FROM information_schema.schemata
WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
ORDER BY schema_name";

/// List tables in a schema with row estimates and size
pub const LIST_TABLES: &str = "\
SELECT
    t.table_name,
    COALESCE(c.reltuples::bigint, 0) AS row_estimate,
    COALESCE(pg_size_pretty(pg_total_relation_size(quote_ident(t.table_schema) || '.' || quote_ident(t.table_name))), '0 bytes') AS total_size
FROM information_schema.tables t
LEFT JOIN pg_catalog.pg_class c
    ON c.relname = t.table_name
    AND c.relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = t.table_schema)
WHERE t.table_schema = $1
    AND t.table_type = 'BASE TABLE'
ORDER BY t.table_name";

/// List views in a schema
pub const LIST_VIEWS: &str = "\
SELECT table_name, view_definition
FROM information_schema.views
WHERE table_schema = $1
ORDER BY table_name";

/// List columns for a table
pub const LIST_COLUMNS: &str = "\
SELECT
    c.column_name,
    c.data_type,
    c.is_nullable,
    c.column_default,
    CASE WHEN pk.column_name IS NOT NULL THEN 'YES' ELSE 'NO' END AS is_primary_key
FROM information_schema.columns c
LEFT JOIN (
    SELECT ku.column_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage ku
        ON tc.constraint_name = ku.constraint_name
        AND tc.table_schema = ku.table_schema
    WHERE tc.constraint_type = 'PRIMARY KEY'
        AND tc.table_schema = $1
        AND tc.table_name = $2
) pk ON pk.column_name = c.column_name
WHERE c.table_schema = $1
    AND c.table_name = $2
ORDER BY c.ordinal_position";

/// List indexes for a table
pub const LIST_INDEXES: &str = "\
SELECT
    i.relname AS index_name,
    ix.indisunique AS is_unique,
    ix.indisprimary AS is_primary,
    pg_get_indexdef(ix.indexrelid) AS definition
FROM pg_catalog.pg_index ix
JOIN pg_catalog.pg_class i ON i.oid = ix.indexrelid
JOIN pg_catalog.pg_class t ON t.oid = ix.indrelid
JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace
WHERE n.nspname = $1
    AND t.relname = $2
ORDER BY i.relname";

/// List constraints for a table
pub const LIST_CONSTRAINTS: &str = "\
SELECT
    tc.constraint_name,
    tc.constraint_type,
    STRING_AGG(kcu.column_name, ', ' ORDER BY kcu.ordinal_position) AS columns
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu
    ON tc.constraint_name = kcu.constraint_name
    AND tc.table_schema = kcu.table_schema
WHERE tc.table_schema = $1
    AND tc.table_name = $2
GROUP BY tc.constraint_name, tc.constraint_type
ORDER BY tc.constraint_type, tc.constraint_name";

/// List functions in a schema
pub const LIST_FUNCTIONS: &str = "\
SELECT
    p.proname AS function_name,
    pg_get_function_result(p.oid) AS return_type,
    pg_get_function_arguments(p.oid) AS arguments
FROM pg_catalog.pg_proc p
JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = $1
    AND p.prokind = 'f'
ORDER BY p.proname";
