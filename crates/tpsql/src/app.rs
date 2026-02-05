//! Main TUI application for tpsql

use crate::config::TpsqlConfig;
use crate::db::{
    db_worker, ColumnInfo, DbCommand, DbResult, FunctionInfo, IndexInfo, ResultColumn, ResultRow,
    TableInfo, ViewInfo,
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use tokio::sync::mpsc;
use tui_core::event::{Event, KeyEventExt};
use tui_core::widgets::{
    centered_rect, focused_block, titled_block, HelpPanel, KeyBinding, StatusBar, TextInput,
};
use tui_core::{
    App, AppResult, Block, Color, Frame, Line, List, ListItem, Modifier, Paragraph, Span, Style,
    Wrap,
};

/// Application view state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    ConnectionInput,
    QueryEditor,
    Results,
    SchemaExplorer,
}

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// Which field is focused in connection form
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnField {
    Host,
    Port,
    Database,
    User,
    Password,
}

/// Active panel when in query mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryPanel {
    Editor,
    Results,
}

/// Schema explorer tree node types
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum SchemaNode {
    Schema {
        name: String,
        expanded: bool,
    },
    TablesHeader {
        schema: String,
        expanded: bool,
    },
    Table {
        schema: String,
        name: String,
        row_estimate: i64,
        total_size: String,
        expanded: bool,
    },
    Column {
        name: String,
        data_type: String,
        is_nullable: bool,
        is_primary_key: bool,
    },
    IndexesHeader {
        schema: String,
        table: String,
        expanded: bool,
    },
    Index {
        name: String,
        definition: String,
    },
    ViewsHeader {
        schema: String,
        expanded: bool,
    },
    View {
        name: String,
    },
    FunctionsHeader {
        schema: String,
        expanded: bool,
    },
    Function {
        name: String,
        return_type: String,
        arguments: String,
    },
}

/// Main tpsql application
pub struct TpsqlApp {
    config: TpsqlConfig,
    view: View,
    show_help: bool,

    // Connection state
    connection_state: ConnectionState,

    // Channels for background worker
    cmd_tx: mpsc::UnboundedSender<DbCommand>,
    result_rx: mpsc::UnboundedReceiver<DbResult>,

    // Connection form fields
    conn_host: TextInput,
    conn_port: TextInput,
    conn_db: TextInput,
    conn_user: TextInput,
    conn_password: TextInput,
    conn_focus: ConnField,

    // Query editor state
    query_lines: Vec<String>,
    query_cursor_line: usize,
    query_cursor_col: usize,
    query_panel: QueryPanel,
    executing: bool,

    // Query history
    query_history: Vec<String>,
    history_index: Option<usize>,

    // Results state
    result_columns: Vec<ResultColumn>,
    result_rows: Vec<ResultRow>,
    result_scroll_row: usize,
    result_scroll_col: usize,
    result_message: Option<String>,

    // Schema explorer state
    schema_tree: Vec<SchemaNode>,
    schema_selected: usize,
    schema_loading: bool,

    // Status message
    status: String,
    server_version: String,
}

impl TpsqlApp {
    pub fn new(config: TpsqlConfig) -> Result<Self> {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (result_tx, result_rx) = mpsc::unbounded_channel();

        // Spawn background database worker
        tokio::spawn(async move {
            db_worker(cmd_rx, result_tx).await;
        });

        let app = Self {
            conn_host: TextInput::new()
                .placeholder("localhost")
                .value(&config.connection.host)
                .focused(true),
            conn_port: TextInput::new()
                .placeholder("5432")
                .value(config.connection.port.to_string()),
            conn_db: TextInput::new()
                .placeholder("postgres")
                .value(&config.connection.dbname),
            conn_user: TextInput::new()
                .placeholder("postgres")
                .value(&config.connection.user),
            conn_password: TextInput::new()
                .placeholder("Password...")
                .masked(true),
            conn_focus: ConnField::Host,
            config,
            view: View::ConnectionInput,
            show_help: false,
            connection_state: ConnectionState::Disconnected,
            cmd_tx,
            result_rx,
            query_lines: vec![String::new()],
            query_cursor_line: 0,
            query_cursor_col: 0,
            query_panel: QueryPanel::Editor,
            executing: false,
            query_history: vec![],
            history_index: None,
            result_columns: vec![],
            result_rows: vec![],
            result_scroll_row: 0,
            result_scroll_col: 0,
            result_message: None,
            schema_tree: vec![],
            schema_selected: 0,
            schema_loading: false,
            status: "Enter connection details and press Enter to connect".to_string(),
            server_version: String::new(),
        };

        Ok(app)
    }

    /// Build DSN from connection form fields
    fn build_dsn(&self) -> String {
        let host = self.conn_host.get_value();
        let port = self.conn_port.get_value();
        let db = self.conn_db.get_value();
        let user = self.conn_user.get_value();
        let password = self.conn_password.get_value();

        let host = if host.is_empty() { "localhost" } else { host };
        let port = if port.is_empty() { "5432" } else { port };
        let db = if db.is_empty() { "postgres" } else { db };
        let user = if user.is_empty() { "postgres" } else { user };

        let mut dsn = format!("host={host} port={port} dbname={db} user={user}");
        if !password.is_empty() {
            dsn.push_str(&format!(" password={password}"));
        }
        dsn
    }

    /// Connect using current form values
    fn connect(&mut self) {
        let dsn = self.build_dsn();
        self.connection_state = ConnectionState::Connecting;
        self.status = "Connecting...".to_string();
        let _ = self.cmd_tx.send(DbCommand::Connect(dsn));
    }

    /// Execute the current query
    fn execute_query(&mut self) {
        let sql = self.query_lines.join("\n").trim().to_string();
        if sql.is_empty() {
            self.status = "Empty query".to_string();
            return;
        }

        // Add to history
        if self.query_history.last().map(|h| h.as_str()) != Some(&sql) {
            self.query_history.push(sql.clone());
            if self.query_history.len() > self.config.history_size {
                self.query_history.remove(0);
            }
        }
        self.history_index = None;

        self.executing = true;
        self.status = "Executing query...".to_string();
        self.result_message = None;
        let _ = self.cmd_tx.send(DbCommand::ExecuteQuery(sql));
    }

    /// Navigate to previous query in history
    fn history_prev(&mut self) {
        if self.query_history.is_empty() {
            return;
        }
        let idx = match self.history_index {
            Some(i) => i.saturating_sub(1),
            None => self.query_history.len() - 1,
        };
        self.history_index = Some(idx);
        self.load_history_entry(idx);
    }

    /// Navigate to next query in history
    fn history_next(&mut self) {
        if self.query_history.is_empty() {
            return;
        }
        if let Some(i) = self.history_index {
            if i + 1 < self.query_history.len() {
                self.history_index = Some(i + 1);
                self.load_history_entry(i + 1);
            } else {
                // Past the end — clear to empty
                self.history_index = None;
                self.query_lines = vec![String::new()];
                self.query_cursor_line = 0;
                self.query_cursor_col = 0;
            }
        }
    }

    fn load_history_entry(&mut self, idx: usize) {
        if let Some(entry) = self.query_history.get(idx) {
            self.query_lines = entry.lines().map(|l| l.to_string()).collect();
            if self.query_lines.is_empty() {
                self.query_lines.push(String::new());
            }
            self.query_cursor_line = self.query_lines.len() - 1;
            self.query_cursor_col = self.query_lines[self.query_cursor_line].len();
        }
    }

    /// Process results from the background worker
    fn process_db_results(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.handle_db_result(result);
        }
    }

    fn handle_db_result(&mut self, result: DbResult) {
        match result {
            DbResult::Connected(version) => {
                self.connection_state = ConnectionState::Connected;
                self.server_version = version.clone();
                self.status = format!("Connected: {}", truncate_str(&version, 60));
                self.view = View::QueryEditor;
                self.query_panel = QueryPanel::Editor;
            }
            DbResult::ConnectionError(e) => {
                self.connection_state = ConnectionState::Error;
                self.status = format!("Connection failed: {e}");
            }
            DbResult::Disconnected => {
                self.connection_state = ConnectionState::Disconnected;
                self.status = "Disconnected".to_string();
                self.view = View::ConnectionInput;
            }
            DbResult::QueryRows {
                columns,
                rows,
                execution_time,
            } => {
                let count = rows.len();
                self.result_columns = columns;
                self.result_rows = rows;
                self.result_scroll_row = 0;
                self.result_scroll_col = 0;
                self.executing = false;
                self.result_message = None;
                self.status = format!("{count} rows ({:.1}ms)", execution_time.as_secs_f64() * 1000.0);
                self.query_panel = QueryPanel::Results;
            }
            DbResult::QueryExecuted {
                affected_rows,
                execution_time,
            } => {
                self.result_columns.clear();
                self.result_rows.clear();
                self.executing = false;
                self.result_message = Some(format!(
                    "{affected_rows} row(s) affected ({:.1}ms)",
                    execution_time.as_secs_f64() * 1000.0
                ));
                self.status = self.result_message.clone().unwrap();
            }
            DbResult::QueryError(e) => {
                self.executing = false;
                self.result_message = Some(format!("ERROR: {e}"));
                self.status = format!("Query error: {}", truncate_str(&e, 60));
            }
            DbResult::Schemas(schemas) => {
                self.schema_loading = false;
                self.schema_tree = schemas
                    .into_iter()
                    .map(|s| SchemaNode::Schema {
                        name: s.name,
                        expanded: false,
                    })
                    .collect();
                self.schema_selected = 0;
                self.status = format!("{} schemas", self.schema_tree.len());
            }
            DbResult::Tables(schema, tables) => {
                self.insert_table_nodes(&schema, &tables);
            }
            DbResult::Views(schema, views) => {
                self.insert_view_nodes(&schema, &views);
            }
            DbResult::Columns(schema, table, columns) => {
                self.insert_column_nodes(&schema, &table, &columns);
            }
            DbResult::Indexes(schema, table, indexes) => {
                self.insert_index_nodes(&schema, &table, &indexes);
            }
            DbResult::Constraints(_schema, _table, _constraints) => {
                // Constraints are shown inline with columns for now
            }
            DbResult::Functions(schema, functions) => {
                self.insert_function_nodes(&schema, &functions);
            }
            DbResult::Error(e) => {
                self.schema_loading = false;
                self.status = format!("Error: {e}");
            }
        }
    }

    // --- Schema tree manipulation ---

    fn insert_table_nodes(&mut self, schema: &str, tables: &[TableInfo]) {
        // Find the TablesHeader for this schema and insert after it
        if let Some(pos) = self.schema_tree.iter().position(|n| {
            matches!(n, SchemaNode::TablesHeader { schema: s, .. } if s == schema)
        }) {
            // Remove old table entries
            let mut end = pos + 1;
            while end < self.schema_tree.len() {
                match &self.schema_tree[end] {
                    SchemaNode::Table { schema: s, .. } if s == schema => end += 1,
                    SchemaNode::Column { .. } => end += 1,
                    SchemaNode::IndexesHeader { schema: s, .. } if s == schema => end += 1,
                    SchemaNode::Index { .. } => end += 1,
                    _ => break,
                }
            }
            self.schema_tree.drain(pos + 1..end);

            // Insert new table entries
            let new_nodes: Vec<SchemaNode> = tables
                .iter()
                .map(|t| SchemaNode::Table {
                    schema: schema.to_string(),
                    name: t.name.clone(),
                    row_estimate: t.row_estimate,
                    total_size: t.total_size.clone(),
                    expanded: false,
                })
                .collect();
            let insert_pos = pos + 1;
            for (i, node) in new_nodes.into_iter().enumerate() {
                self.schema_tree.insert(insert_pos + i, node);
            }
        }
    }

    fn insert_view_nodes(&mut self, schema: &str, views: &[ViewInfo]) {
        if let Some(pos) = self.schema_tree.iter().position(|n| {
            matches!(n, SchemaNode::ViewsHeader { schema: s, .. } if s == schema)
        }) {
            // Remove old view entries
            let mut end = pos + 1;
            while end < self.schema_tree.len() {
                match &self.schema_tree[end] {
                    SchemaNode::View { .. } => end += 1,
                    _ => break,
                }
            }
            self.schema_tree.drain(pos + 1..end);

            let insert_pos = pos + 1;
            for (i, v) in views.iter().enumerate() {
                self.schema_tree.insert(
                    insert_pos + i,
                    SchemaNode::View {
                        name: v.name.clone(),
                    },
                );
            }
        }
    }

    fn insert_column_nodes(&mut self, schema: &str, table: &str, columns: &[ColumnInfo]) {
        if let Some(pos) = self.schema_tree.iter().position(|n| {
            matches!(n, SchemaNode::Table { schema: s, name: t, .. } if s == schema && t == table)
        }) {
            // Remove old columns and indexes header/indexes for this table
            let mut end = pos + 1;
            while end < self.schema_tree.len() {
                match &self.schema_tree[end] {
                    SchemaNode::Column { .. } => end += 1,
                    SchemaNode::IndexesHeader { schema: s, table: t, .. }
                        if s == schema && t == table =>
                    {
                        end += 1
                    }
                    SchemaNode::Index { .. } => end += 1,
                    _ => break,
                }
            }
            self.schema_tree.drain(pos + 1..end);

            let mut insert_pos = pos + 1;
            for col in columns {
                self.schema_tree.insert(
                    insert_pos,
                    SchemaNode::Column {
                        name: col.name.clone(),
                        data_type: col.data_type.clone(),
                        is_nullable: col.is_nullable,
                        is_primary_key: col.is_primary_key,
                    },
                );
                insert_pos += 1;
            }

            // Add indexes header
            self.schema_tree.insert(
                insert_pos,
                SchemaNode::IndexesHeader {
                    schema: schema.to_string(),
                    table: table.to_string(),
                    expanded: false,
                },
            );

            // Also fetch indexes
            let _ = self.cmd_tx.send(DbCommand::FetchIndexes(
                schema.to_string(),
                table.to_string(),
            ));
        }
    }

    fn insert_index_nodes(&mut self, schema: &str, table: &str, indexes: &[IndexInfo]) {
        if let Some(pos) = self.schema_tree.iter().position(|n| {
            matches!(n, SchemaNode::IndexesHeader { schema: s, table: t, expanded: true, .. } if s == schema && t == table)
        }) {
            // Remove old index entries
            let mut end = pos + 1;
            while end < self.schema_tree.len() {
                match &self.schema_tree[end] {
                    SchemaNode::Index { .. } => end += 1,
                    _ => break,
                }
            }
            self.schema_tree.drain(pos + 1..end);

            let insert_pos = pos + 1;
            for (i, idx) in indexes.iter().enumerate() {
                self.schema_tree.insert(
                    insert_pos + i,
                    SchemaNode::Index {
                        name: idx.name.clone(),
                        definition: idx.definition.clone(),
                    },
                );
            }
        }
    }

    fn insert_function_nodes(&mut self, schema: &str, functions: &[FunctionInfo]) {
        if let Some(pos) = self.schema_tree.iter().position(|n| {
            matches!(n, SchemaNode::FunctionsHeader { schema: s, .. } if s == schema)
        }) {
            // Remove old function entries
            let mut end = pos + 1;
            while end < self.schema_tree.len() {
                match &self.schema_tree[end] {
                    SchemaNode::Function { .. } => end += 1,
                    _ => break,
                }
            }
            self.schema_tree.drain(pos + 1..end);

            let insert_pos = pos + 1;
            for (i, f) in functions.iter().enumerate() {
                self.schema_tree.insert(
                    insert_pos + i,
                    SchemaNode::Function {
                        name: f.name.clone(),
                        return_type: f.return_type.clone(),
                        arguments: f.arguments.clone(),
                    },
                );
            }
        }
    }

    /// Get indentation level for a schema node
    fn node_indent(node: &SchemaNode) -> usize {
        match node {
            SchemaNode::Schema { .. } => 0,
            SchemaNode::TablesHeader { .. }
            | SchemaNode::ViewsHeader { .. }
            | SchemaNode::FunctionsHeader { .. } => 1,
            SchemaNode::Table { .. } | SchemaNode::View { .. } | SchemaNode::Function { .. } => 2,
            SchemaNode::Column { .. } | SchemaNode::IndexesHeader { .. } => 3,
            SchemaNode::Index { .. } => 4,
        }
    }

    /// Toggle expand/collapse on current schema node
    fn toggle_schema_node(&mut self) {
        if self.schema_tree.is_empty() {
            return;
        }
        let idx = self.schema_selected;
        match &self.schema_tree[idx] {
            SchemaNode::Schema { name, expanded } => {
                let expanded = !expanded;
                let name = name.clone();
                self.schema_tree[idx] = SchemaNode::Schema {
                    name: name.clone(),
                    expanded,
                };
                if expanded {
                    // Insert headers for tables, views, functions
                    let insert_pos = idx + 1;
                    self.schema_tree.insert(
                        insert_pos,
                        SchemaNode::FunctionsHeader {
                            schema: name.clone(),
                            expanded: false,
                        },
                    );
                    self.schema_tree.insert(
                        insert_pos,
                        SchemaNode::ViewsHeader {
                            schema: name.clone(),
                            expanded: false,
                        },
                    );
                    self.schema_tree.insert(
                        insert_pos,
                        SchemaNode::TablesHeader {
                            schema: name,
                            expanded: false,
                        },
                    );
                } else {
                    // Remove all children
                    self.collapse_children(idx);
                }
            }
            SchemaNode::TablesHeader {
                schema, expanded, ..
            } => {
                let expanded = !expanded;
                let schema = schema.clone();
                self.schema_tree[idx] = SchemaNode::TablesHeader {
                    schema: schema.clone(),
                    expanded,
                };
                if expanded {
                    let _ = self.cmd_tx.send(DbCommand::FetchTables(schema));
                } else {
                    self.collapse_children(idx);
                }
            }
            SchemaNode::Table {
                schema,
                name,
                row_estimate,
                total_size,
                expanded,
            } => {
                let expanded = !expanded;
                let schema = schema.clone();
                let name = name.clone();
                let row_estimate = *row_estimate;
                let total_size = total_size.clone();
                self.schema_tree[idx] = SchemaNode::Table {
                    schema: schema.clone(),
                    name: name.clone(),
                    row_estimate,
                    total_size,
                    expanded,
                };
                if expanded {
                    let _ = self.cmd_tx.send(DbCommand::FetchColumns(
                        schema.clone(),
                        name.clone(),
                    ));
                } else {
                    self.collapse_children(idx);
                }
            }
            SchemaNode::IndexesHeader {
                schema,
                table,
                expanded,
            } => {
                let expanded = !expanded;
                let schema = schema.clone();
                let table = table.clone();
                self.schema_tree[idx] = SchemaNode::IndexesHeader {
                    schema: schema.clone(),
                    table: table.clone(),
                    expanded,
                };
                if expanded {
                    let _ = self
                        .cmd_tx
                        .send(DbCommand::FetchIndexes(schema, table));
                } else {
                    self.collapse_children(idx);
                }
            }
            SchemaNode::ViewsHeader {
                schema, expanded, ..
            } => {
                let expanded = !expanded;
                let schema = schema.clone();
                self.schema_tree[idx] = SchemaNode::ViewsHeader {
                    schema: schema.clone(),
                    expanded,
                };
                if expanded {
                    let _ = self.cmd_tx.send(DbCommand::FetchViews(schema));
                } else {
                    self.collapse_children(idx);
                }
            }
            SchemaNode::FunctionsHeader {
                schema, expanded, ..
            } => {
                let expanded = !expanded;
                let schema = schema.clone();
                self.schema_tree[idx] = SchemaNode::FunctionsHeader {
                    schema: schema.clone(),
                    expanded,
                };
                if expanded {
                    let _ = self.cmd_tx.send(DbCommand::FetchFunctions(schema));
                } else {
                    self.collapse_children(idx);
                }
            }
            // Leaf nodes — no toggle
            _ => {}
        }
    }

    /// Collapse a schema node (remove children with deeper indentation)
    fn collapse_schema_node(&mut self) {
        if self.schema_tree.is_empty() {
            return;
        }
        let idx = self.schema_selected;
        let current_indent = Self::node_indent(&self.schema_tree[idx]);

        if current_indent == 0 {
            // Already at top level, collapse if expanded
            if let SchemaNode::Schema { expanded: true, .. } = &self.schema_tree[idx] {
                self.toggle_schema_node();
            }
            return;
        }

        // Navigate to parent
        let mut parent = idx;
        while parent > 0 {
            parent -= 1;
            if Self::node_indent(&self.schema_tree[parent]) < current_indent {
                self.schema_selected = parent;
                return;
            }
        }
        self.schema_selected = 0;
    }

    /// Remove all children of node at idx (nodes with greater indentation)
    fn collapse_children(&mut self, idx: usize) {
        let parent_indent = Self::node_indent(&self.schema_tree[idx]);
        let mut end = idx + 1;
        while end < self.schema_tree.len() {
            if Self::node_indent(&self.schema_tree[end]) > parent_indent {
                end += 1;
            } else {
                break;
            }
        }
        self.schema_tree.drain(idx + 1..end);
    }

    // --- Input handling ---

    fn handle_body_input(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                if self.query_lines.is_empty() {
                    self.query_lines.push(String::new());
                }
                let line = &mut self.query_lines[self.query_cursor_line];
                line.insert(self.query_cursor_col, c);
                self.query_cursor_col += 1;
            }
            KeyCode::Enter => {
                let current_line = self.query_lines[self.query_cursor_line].clone();
                let (before, after) = current_line.split_at(self.query_cursor_col);
                self.query_lines[self.query_cursor_line] = before.to_string();
                self.query_lines
                    .insert(self.query_cursor_line + 1, after.to_string());
                self.query_cursor_line += 1;
                self.query_cursor_col = 0;
            }
            KeyCode::Backspace => {
                if self.query_cursor_col > 0 {
                    let line = &mut self.query_lines[self.query_cursor_line];
                    line.remove(self.query_cursor_col - 1);
                    self.query_cursor_col -= 1;
                } else if self.query_cursor_line > 0 {
                    let current = self.query_lines.remove(self.query_cursor_line);
                    self.query_cursor_line -= 1;
                    self.query_cursor_col = self.query_lines[self.query_cursor_line].len();
                    self.query_lines[self.query_cursor_line].push_str(&current);
                }
            }
            KeyCode::Delete => {
                let line = &mut self.query_lines[self.query_cursor_line];
                if self.query_cursor_col < line.len() {
                    line.remove(self.query_cursor_col);
                } else if self.query_cursor_line < self.query_lines.len() - 1 {
                    let next = self.query_lines.remove(self.query_cursor_line + 1);
                    self.query_lines[self.query_cursor_line].push_str(&next);
                }
            }
            KeyCode::Up => {
                if self.query_cursor_line > 0 {
                    self.query_cursor_line -= 1;
                    self.query_cursor_col = self
                        .query_cursor_col
                        .min(self.query_lines[self.query_cursor_line].len());
                }
            }
            KeyCode::Down => {
                if self.query_cursor_line < self.query_lines.len() - 1 {
                    self.query_cursor_line += 1;
                    self.query_cursor_col = self
                        .query_cursor_col
                        .min(self.query_lines[self.query_cursor_line].len());
                }
            }
            KeyCode::Left => {
                if self.query_cursor_col > 0 {
                    self.query_cursor_col -= 1;
                } else if self.query_cursor_line > 0 {
                    self.query_cursor_line -= 1;
                    self.query_cursor_col = self.query_lines[self.query_cursor_line].len();
                }
            }
            KeyCode::Right => {
                let line_len = self
                    .query_lines
                    .get(self.query_cursor_line)
                    .map(|l| l.len())
                    .unwrap_or(0);
                if self.query_cursor_col < line_len {
                    self.query_cursor_col += 1;
                } else if self.query_cursor_line < self.query_lines.len() - 1 {
                    self.query_cursor_line += 1;
                    self.query_cursor_col = 0;
                }
            }
            KeyCode::Home => {
                self.query_cursor_col = 0;
            }
            KeyCode::End => {
                self.query_cursor_col = self
                    .query_lines
                    .get(self.query_cursor_line)
                    .map(|l| l.len())
                    .unwrap_or(0);
            }
            _ => {}
        }
    }

    // --- Rendering ---

    fn render_connection_input(&mut self, frame: &mut Frame) {
        let area = centered_rect(50, 50, frame.area());
        frame.render_widget(ratatui::widgets::Clear, area);

        let block = focused_block("PostgreSQL Connection");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // Host
                Constraint::Length(3), // Port
                Constraint::Length(3), // Database
                Constraint::Length(3), // User
                Constraint::Length(3), // Password
                Constraint::Length(1), // Spacing
                Constraint::Length(1), // Hint
            ])
            .split(inner);

        self.render_conn_field("Host:", &self.conn_host.clone(), ConnField::Host, chunks[0], frame);
        self.render_conn_field("Port:", &self.conn_port.clone(), ConnField::Port, chunks[1], frame);
        self.render_conn_field(
            "Database:",
            &self.conn_db.clone(),
            ConnField::Database,
            chunks[2],
            frame,
        );
        self.render_conn_field("User:", &self.conn_user.clone(), ConnField::User, chunks[3], frame);
        self.render_conn_field(
            "Password:",
            &self.conn_password.clone(),
            ConnField::Password,
            chunks[4],
            frame,
        );

        let hint = Paragraph::new("Enter: connect | Tab: next field | Esc: quit")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, chunks[6]);
    }

    fn render_conn_field(
        &self,
        label: &str,
        input: &TextInput,
        field: ConnField,
        area: ratatui::layout::Rect,
        frame: &mut Frame,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(11), Constraint::Min(1)])
            .split(area);

        let label_style = if self.conn_focus == field {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        frame.render_widget(Paragraph::new(label).style(label_style), chunks[0]);

        let border_style = if self.conn_focus == field {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let mut input = input.clone();
        input = input.focused(self.conn_focus == field);
        input.render_with_block(
            chunks[1],
            frame.buffer_mut(),
            Block::bordered().border_style(border_style),
        );
    }

    fn render_query_editor(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let is_focused = self.query_panel == QueryPanel::Editor;
        let block = if is_focused {
            focused_block("SQL Query")
        } else {
            titled_block("SQL Query")
        };
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible_height = inner.height as usize;
        let scroll_offset = if self.query_cursor_line >= visible_height {
            self.query_cursor_line - visible_height + 1
        } else {
            0
        };

        let mut lines: Vec<Line> = Vec::new();
        for (i, line) in self.query_lines.iter().enumerate().skip(scroll_offset) {
            if lines.len() >= visible_height {
                break;
            }

            let line_num = format!("{:>3} ", i + 1);

            if is_focused && i == self.query_cursor_line {
                let cursor_col = self.query_cursor_col.min(line.len());
                let (before, after) = if cursor_col <= line.len() {
                    (&line[..cursor_col], &line[cursor_col..])
                } else {
                    (line.as_str(), "")
                };

                let cursor_char = after.chars().next().unwrap_or(' ');
                let after_cursor = if after.is_empty() {
                    ""
                } else {
                    &after[cursor_char.len_utf8()..]
                };

                lines.push(Line::from(vec![
                    Span::styled(line_num, Style::default().fg(Color::DarkGray)),
                    Span::raw(before),
                    Span::styled(
                        cursor_char.to_string(),
                        Style::default().bg(Color::White).fg(Color::Black),
                    ),
                    Span::raw(after_cursor),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled(line_num, Style::default().fg(Color::DarkGray)),
                    Span::raw(line),
                ]));
            }
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_results(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let is_focused = self.query_panel == QueryPanel::Results;
        let block = if is_focused {
            focused_block("Results")
        } else {
            titled_block("Results")
        };
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if let Some(ref msg) = self.result_message {
            let style = if msg.starts_with("ERROR") {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            frame.render_widget(
                Paragraph::new(msg.as_str()).style(style).wrap(Wrap { trim: false }),
                inner,
            );
            return;
        }

        if self.result_columns.is_empty() {
            frame.render_widget(
                Paragraph::new("No results. Run a query with Ctrl+Enter or F5.")
                    .style(Style::default().fg(Color::DarkGray)),
                inner,
            );
            return;
        }

        // Calculate column widths
        let col_widths: Vec<usize> = self
            .result_columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let header_w = col.name.len();
                let max_data_w = self
                    .result_rows
                    .iter()
                    .map(|row| {
                        row.get(i)
                            .map(|v| v.as_deref().unwrap_or("NULL").len())
                            .unwrap_or(4)
                    })
                    .max()
                    .unwrap_or(4);
                header_w.max(max_data_w).clamp(4, 40)
            })
            .collect();

        let visible_width = inner.width as usize;
        let visible_height = inner.height as usize;

        // Build header
        let header_spans: Vec<Span> = self
            .result_columns
            .iter()
            .enumerate()
            .skip(self.result_scroll_col)
            .map(|(i, col)| {
                let w = col_widths[i];
                Span::styled(
                    format!(" {:<width$}", col.name, width = w),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect();

        let mut lines: Vec<Line> = vec![Line::from(header_spans)];

        // Separator
        let sep_width = col_widths
            .iter()
            .skip(self.result_scroll_col)
            .map(|w| w + 1)
            .sum::<usize>()
            .min(visible_width);
        lines.push(Line::styled(
            "─".repeat(sep_width),
            Style::default().fg(Color::DarkGray),
        ));

        // Data rows
        let max_rows = visible_height.saturating_sub(2);
        for row in self
            .result_rows
            .iter()
            .skip(self.result_scroll_row)
            .take(max_rows)
        {
            let row_spans: Vec<Span> = row
                .iter()
                .enumerate()
                .skip(self.result_scroll_col)
                .map(|(i, val)| {
                    let w = col_widths.get(i).copied().unwrap_or(10);
                    match val {
                        Some(v) => Span::raw(format!(" {:<width$}", truncate_str(v, w), width = w)),
                        None => Span::styled(
                            format!(" {:<width$}", "NULL", width = w),
                            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                        ),
                    }
                })
                .collect();
            lines.push(Line::from(row_spans));
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_schema_explorer(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let block = focused_block("Schema Explorer");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.schema_tree.is_empty() {
            let msg = if self.schema_loading {
                "Loading schemas..."
            } else {
                "Press 'r' to load schemas"
            };
            frame.render_widget(
                Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
                inner,
            );
            return;
        }

        let visible_height = inner.height as usize;

        // Scroll to keep selection visible
        let scroll_offset = if self.schema_selected >= visible_height {
            self.schema_selected - visible_height + 1
        } else {
            0
        };

        let items: Vec<ListItem> = self
            .schema_tree
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_height)
            .map(|(i, node)| {
                let indent = Self::node_indent(node);
                let indent_str = "  ".repeat(indent);
                let is_selected = i == self.schema_selected;

                let (icon, label, detail) = match node {
                    SchemaNode::Schema { name, expanded } => {
                        let icon = if *expanded { "▼" } else { "▶" };
                        (icon, name.clone(), String::new())
                    }
                    SchemaNode::TablesHeader { expanded, .. } => {
                        let icon = if *expanded { "▼" } else { "▶" };
                        (icon, "Tables".to_string(), String::new())
                    }
                    SchemaNode::Table {
                        name,
                        row_estimate,
                        total_size,
                        expanded,
                        ..
                    } => {
                        let icon = if *expanded { "▼" } else { "▶" };
                        let detail = format!("~{row_estimate} rows, {total_size}");
                        (icon, name.clone(), detail)
                    }
                    SchemaNode::Column {
                        name,
                        data_type,
                        is_nullable,
                        is_primary_key,
                    } => {
                        let pk = if *is_primary_key { " PK" } else { "" };
                        let null = if *is_nullable { " NULL" } else { " NOT NULL" };
                        let detail = format!("{data_type}{null}{pk}");
                        ("•", name.clone(), detail)
                    }
                    SchemaNode::IndexesHeader { expanded, .. } => {
                        let icon = if *expanded { "▼" } else { "▶" };
                        (icon, "Indexes".to_string(), String::new())
                    }
                    SchemaNode::Index { name, .. } => ("◦", name.clone(), String::new()),
                    SchemaNode::ViewsHeader { expanded, .. } => {
                        let icon = if *expanded { "▼" } else { "▶" };
                        (icon, "Views".to_string(), String::new())
                    }
                    SchemaNode::View { name } => ("◦", name.clone(), String::new()),
                    SchemaNode::FunctionsHeader { expanded, .. } => {
                        let icon = if *expanded { "▼" } else { "▶" };
                        (icon, "Functions".to_string(), String::new())
                    }
                    SchemaNode::Function { name, arguments, .. } => {
                        let detail = format!("({arguments})");
                        ("ƒ", name.clone(), detail)
                    }
                };

                let style = if is_selected {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default()
                };

                let mut spans = vec![
                    Span::raw(indent_str),
                    Span::styled(format!("{icon} "), Style::default().fg(Color::Yellow)),
                    Span::styled(label, style),
                ];
                if !detail.is_empty() {
                    spans.push(Span::styled(
                        format!("  {detail}"),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        frame.render_widget(List::new(items), inner);
    }

    fn render_help(&self, frame: &mut Frame) {
        let area = centered_rect(60, 70, frame.area());

        let help = HelpPanel::new("Help - tpsql")
            .section(
                "Connection",
                vec![
                    KeyBinding::new("Tab/S-Tab", "Cycle fields"),
                    KeyBinding::new("Enter", "Connect"),
                    KeyBinding::new("Esc", "Quit / disconnect"),
                ],
            )
            .section(
                "Query Editor",
                vec![
                    KeyBinding::new("Ctrl+Enter/F5", "Execute query"),
                    KeyBinding::new("Ctrl+P", "Previous query (history)"),
                    KeyBinding::new("Ctrl+N", "Next query (history)"),
                    KeyBinding::new("Tab", "Switch to results"),
                ],
            )
            .section(
                "Results",
                vec![
                    KeyBinding::new("j/k", "Scroll rows"),
                    KeyBinding::new("h/l", "Scroll columns"),
                    KeyBinding::new("e/Tab", "Back to editor"),
                ],
            )
            .section(
                "Schema Explorer",
                vec![
                    KeyBinding::new("j/k", "Navigate"),
                    KeyBinding::new("Enter/l", "Expand"),
                    KeyBinding::new("h/Bksp", "Collapse / parent"),
                    KeyBinding::new("r", "Refresh schemas"),
                ],
            )
            .section(
                "General",
                vec![
                    KeyBinding::new("1-4", "Switch view"),
                    KeyBinding::new("?", "Toggle help"),
                    KeyBinding::new("q/Ctrl+C", "Quit"),
                ],
            );

        frame.render_widget(help, area);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let connection_indicator = match self.connection_state {
            ConnectionState::Disconnected => Span::styled("●", Style::default().fg(Color::Red)),
            ConnectionState::Connecting => Span::styled("●", Style::default().fg(Color::Yellow)),
            ConnectionState::Connected => Span::styled("●", Style::default().fg(Color::Green)),
            ConnectionState::Error => Span::styled("●", Style::default().fg(Color::Red)),
        };

        let view_indicator = match self.view {
            View::ConnectionInput => "Connect",
            View::QueryEditor => "Query",
            View::Results => "Results",
            View::SchemaExplorer => "Schema",
        };

        let status = StatusBar::new()
            .left(connection_indicator)
            .left(Span::styled(
                format!(" {view_indicator} "),
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))
            .left(Span::raw(format!(" {} ", self.status)))
            .key_hint("?", "Help")
            .key_hint("q", "Quit");

        frame.render_widget(status, area);
    }
}

impl Drop for TpsqlApp {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(DbCommand::Disconnect);
    }
}

impl App for TpsqlApp {
    fn name(&self) -> &'static str {
        "tpsql"
    }

    fn tick(&mut self) -> AppResult<()> {
        self.process_db_results();
        Ok(())
    }

    fn handle_event(&mut self, event: Event) -> AppResult<bool> {
        match event {
            Event::Key(key) => {
                // Connection input view
                if self.view == View::ConnectionInput {
                    return self.handle_connection_input(key);
                }

                // Help toggle
                if key.code == KeyCode::Char('?')
                    && !matches!(
                        self.view,
                        View::QueryEditor if self.query_panel == QueryPanel::Editor
                    )
                {
                    self.show_help = !self.show_help;
                    return Ok(false);
                }

                if self.show_help {
                    if key.is_cancel() || key.code == KeyCode::Char('?') {
                        self.show_help = false;
                    }
                    return Ok(false);
                }

                // Global quit
                if key.is_quit() && self.view != View::QueryEditor {
                    return Ok(true);
                }
                if key.is_quit()
                    && self.view == View::QueryEditor
                    && self.query_panel == QueryPanel::Results
                {
                    return Ok(true);
                }

                // Ctrl+C always quits
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(true);
                }

                // View switching with number keys (not in editor)
                if self.query_panel != QueryPanel::Editor || self.view != View::QueryEditor {
                    match key.code {
                        KeyCode::Char('1') => {
                            self.view = View::ConnectionInput;
                            return Ok(false);
                        }
                        KeyCode::Char('2') if self.connection_state == ConnectionState::Connected => {
                            self.view = View::QueryEditor;
                            self.query_panel = QueryPanel::Editor;
                            return Ok(false);
                        }
                        KeyCode::Char('3') if self.connection_state == ConnectionState::Connected => {
                            self.view = View::Results;
                            return Ok(false);
                        }
                        KeyCode::Char('4') if self.connection_state == ConnectionState::Connected => {
                            self.view = View::SchemaExplorer;
                            if self.schema_tree.is_empty() && !self.schema_loading {
                                self.schema_loading = true;
                                let _ = self.cmd_tx.send(DbCommand::FetchSchemas);
                            }
                            return Ok(false);
                        }
                        _ => {}
                    }
                }

                // View-specific handling
                match self.view {
                    View::QueryEditor => self.handle_query_editor(key),
                    View::Results => self.handle_results_view(key),
                    View::SchemaExplorer => self.handle_schema_explorer(key),
                    View::ConnectionInput => Ok(false),
                }
            }
            _ => Ok(false),
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        if self.view == View::ConnectionInput {
            self.render_connection_input(frame);

            // Status bar
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(frame.area());
            self.render_status_bar(frame, chunks[1]);

            if self.show_help {
                self.render_help(frame);
            }
            return;
        }

        if self.view == View::SchemaExplorer {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(frame.area());

            self.render_schema_explorer(frame, chunks[0]);
            self.render_status_bar(frame, chunks[1]);

            if self.show_help {
                self.render_help(frame);
            }
            return;
        }

        // Query/Results layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(frame.area());

        self.render_query_editor(frame, chunks[0]);
        self.render_results(frame, chunks[1]);
        self.render_status_bar(frame, chunks[2]);

        if self.show_help {
            self.render_help(frame);
        }
    }
}

impl TpsqlApp {
    fn handle_connection_input(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> AppResult<bool> {
        match key.code {
            KeyCode::Enter => {
                self.connect();
            }
            KeyCode::Esc => {
                if self.connection_state == ConnectionState::Connected {
                    // Go back to query view instead of quitting
                    self.view = View::QueryEditor;
                } else {
                    return Ok(true);
                }
            }
            KeyCode::Tab => {
                self.conn_focus = match self.conn_focus {
                    ConnField::Host => ConnField::Port,
                    ConnField::Port => ConnField::Database,
                    ConnField::Database => ConnField::User,
                    ConnField::User => ConnField::Password,
                    ConnField::Password => ConnField::Host,
                };
            }
            KeyCode::BackTab => {
                self.conn_focus = match self.conn_focus {
                    ConnField::Host => ConnField::Password,
                    ConnField::Port => ConnField::Host,
                    ConnField::Database => ConnField::Port,
                    ConnField::User => ConnField::Database,
                    ConnField::Password => ConnField::User,
                };
            }
            _ => {
                match self.conn_focus {
                    ConnField::Host => self.conn_host.handle_key(key),
                    ConnField::Port => self.conn_port.handle_key(key),
                    ConnField::Database => self.conn_db.handle_key(key),
                    ConnField::User => self.conn_user.handle_key(key),
                    ConnField::Password => self.conn_password.handle_key(key),
                };
            }
        }
        Ok(false)
    }

    fn handle_query_editor(&mut self, key: crossterm::event::KeyEvent) -> AppResult<bool> {
        match self.query_panel {
            QueryPanel::Editor => {
                // Execute query: Ctrl+Enter or F5
                if key.code == KeyCode::F(5)
                    || (key.code == KeyCode::Enter
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    self.execute_query();
                    return Ok(false);
                }

                // History navigation
                if key.code == KeyCode::Char('p')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    self.history_prev();
                    return Ok(false);
                }
                if key.code == KeyCode::Char('n')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    self.history_next();
                    return Ok(false);
                }

                // Esc goes to results panel
                if key.code == KeyCode::Esc {
                    self.query_panel = QueryPanel::Results;
                    return Ok(false);
                }

                // Tab switches to results
                if key.code == KeyCode::Tab {
                    self.query_panel = QueryPanel::Results;
                    return Ok(false);
                }

                // All other keys go to body editor
                self.handle_body_input(key);
            }
            QueryPanel::Results => {
                // Navigation in results
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if self.result_scroll_row + 1 < self.result_rows.len() {
                            self.result_scroll_row += 1;
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.result_scroll_row = self.result_scroll_row.saturating_sub(1);
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        if self.result_scroll_col + 1 < self.result_columns.len() {
                            self.result_scroll_col += 1;
                        }
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        self.result_scroll_col = self.result_scroll_col.saturating_sub(1);
                    }
                    KeyCode::PageDown => {
                        self.result_scroll_row = (self.result_scroll_row + 20)
                            .min(self.result_rows.len().saturating_sub(1));
                    }
                    KeyCode::PageUp => {
                        self.result_scroll_row = self.result_scroll_row.saturating_sub(20);
                    }
                    KeyCode::Char('e') | KeyCode::Tab => {
                        self.query_panel = QueryPanel::Editor;
                    }
                    KeyCode::Esc => {
                        self.query_panel = QueryPanel::Editor;
                    }
                    _ => {}
                }
            }
        }
        Ok(false)
    }

    fn handle_results_view(&mut self, key: crossterm::event::KeyEvent) -> AppResult<bool> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.result_scroll_row + 1 < self.result_rows.len() {
                    self.result_scroll_row += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.result_scroll_row = self.result_scroll_row.saturating_sub(1);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.result_scroll_col + 1 < self.result_columns.len() {
                    self.result_scroll_col += 1;
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.result_scroll_col = self.result_scroll_col.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.result_scroll_row = (self.result_scroll_row + 20)
                    .min(self.result_rows.len().saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.result_scroll_row = self.result_scroll_row.saturating_sub(20);
            }
            KeyCode::Char('e') => {
                self.view = View::QueryEditor;
                self.query_panel = QueryPanel::Editor;
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_schema_explorer(&mut self, key: crossterm::event::KeyEvent) -> AppResult<bool> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.schema_selected + 1 < self.schema_tree.len() {
                    self.schema_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.schema_selected = self.schema_selected.saturating_sub(1);
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                self.toggle_schema_node();
            }
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                self.collapse_schema_node();
            }
            KeyCode::Char('r') => {
                self.schema_tree.clear();
                self.schema_selected = 0;
                self.schema_loading = true;
                let _ = self.cmd_tx.send(DbCommand::FetchSchemas);
            }
            _ => {}
        }
        Ok(false)
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
