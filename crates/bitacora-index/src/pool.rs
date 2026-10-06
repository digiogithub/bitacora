use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::{Condvar, Mutex};
use rusqlite::{Connection, OpenFlags};

use crate::Error;
use crate::schema::apply_read_pragmas;

/// A bounded pool of read-only SQLite connections (WAL allows many readers beside the writer).
///
/// Connections are opened lazily up to `max` and reused. Cloning the pool shares it.
#[derive(Debug, Clone)]
pub struct ReaderPool {
    inner: Arc<PoolInner>,
}

#[derive(Debug)]
struct PoolInner {
    path: PathBuf,
    max: usize,
    state: Mutex<PoolState>,
    available: Condvar,
}

#[derive(Debug, Default)]
struct PoolState {
    idle: Vec<Connection>,
    /// Connections currently open (idle + checked out).
    open: usize,
}

impl ReaderPool {
    /// Create an empty pool for the database at `path`, holding at most `max` (>= 1) connections.
    pub fn new(path: &Path, max: usize) -> Self {
        Self {
            inner: Arc::new(PoolInner {
                path: path.to_owned(),
                max: max.max(1),
                state: Mutex::new(PoolState::default()),
                available: Condvar::new(),
            }),
        }
    }

    /// Check out a read-only connection, opening one if the pool is below its limit and
    /// otherwise blocking until one is returned.
    pub fn get(&self) -> Result<PooledReader, Error> {
        let mut state = self.inner.state.lock();
        loop {
            if let Some(conn) = state.idle.pop() {
                return Ok(self.wrap(conn));
            }
            if state.open < self.inner.max {
                state.open += 1;
                drop(state);
                return match self.open_one() {
                    Ok(conn) => Ok(self.wrap(conn)),
                    Err(e) => {
                        self.inner.state.lock().open -= 1;
                        self.inner.available.notify_one();
                        Err(e)
                    }
                };
            }
            self.inner.available.wait(&mut state);
        }
    }

    /// Number of connections currently open (idle or checked out).
    pub fn open_connections(&self) -> usize {
        self.inner.state.lock().open
    }

    /// Drop every idle connection (e.g. before deleting the database file).
    pub fn clear_idle(&self) {
        let mut state = self.inner.state.lock();
        let n = state.idle.len();
        state.idle.clear();
        state.open -= n;
    }

    fn open_one(&self) -> Result<Connection, Error> {
        let conn = Connection::open_with_flags(
            &self.inner.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        apply_read_pragmas(&conn)?;
        Ok(conn)
    }

    fn wrap(&self, conn: Connection) -> PooledReader {
        PooledReader {
            conn: Some(conn),
            pool: self.clone(),
        }
    }
}

/// A read-only connection checked out of a [`ReaderPool`]; returned on drop.
#[derive(Debug)]
pub struct PooledReader {
    conn: Option<Connection>,
    pool: ReaderPool,
}

impl std::ops::Deref for PooledReader {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        match &self.conn {
            Some(c) => c,
            // `conn` is only taken in `drop`, so a live guard always has one.
            None => unreachable!("pooled reader used after drop"),
        }
    }
}

impl Drop for PooledReader {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.inner.state.lock().idle.push(conn);
            self.pool.inner.available.notify_one();
        }
    }
}
