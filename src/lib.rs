//! ClickHouse support for the `r2d2` connection pool.
#![warn(missing_docs)]

pub use clickhouse;
pub use r2d2;
use std::sync::Arc;
pub use tokio;

use clickhouse::{
    error::{Error, Result},
    Client,
};
use tokio::runtime::Runtime;

#[allow(missing_docs)]
pub struct ClickHouseConnection {
    client: Client,
    rt: Arc<Runtime>,
}

///An `r2d2::ManageConnection` for `clickhouse::Client`
#[derive(Clone)]
pub struct ClickHouseConnectionManager {
    client: Client,
    rt: Arc<Runtime>,
}

impl ClickHouseConnectionManager {
    /// Create a new ClickHouse Connection Manager based on specified parameters
    pub fn new(
        url: String,
        username: String,
        password: String,
        database: String,
    ) -> ClickHouseConnectionManager {
        ClickHouseConnectionManager {
            client: Client::default()
                .with_url(url)
                .with_user(username)
                .with_password(password)
                .with_database(database),
            rt: Arc::new(Runtime::new().unwrap()),
        }
    }
}

impl r2d2::ManageConnection for ClickHouseConnectionManager {
    type Connection = ClickHouseConnection;
    type Error = Error;
    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok(ClickHouseConnection {
            rt: self.rt.clone(),
            client: self.client.clone(),
        })
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        let _ = conn
            .rt
            .block_on(async { conn.client.query("SELECT 1").fetch_all::<String>().await })
            .expect("Connection error");

        Ok(())
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        false // Clickhouse-rs doesn't provide a way to check if a connection is broken
    }
}

#[cfg(test)]
mod test {
    use crate::ClickHouseConnectionManager;
    use std::env;

    fn get_test_config() -> (String, String, String, String) {
        (
            env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".to_string()),
            env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".to_string()),
            env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "".to_string()),
            env::var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "default".to_string()),
        )
    }

    #[test]
    fn query_pool() {
        let (url, username, password, database) = get_test_config();

        let manager = ClickHouseConnectionManager::new(url, username, password, database);

        let pool = r2d2::Pool::builder()
            .max_size(5)
            .build(manager)
            .expect("Failed to create connection pool");

        let mut tasks = vec![];

        for _ in 0..4 {
            let pool = pool.clone();
            let th = std::thread::spawn(move || {
                let conn = pool.get().expect("Failed to acquire connection from pool");

                let _ = conn
                    .rt
                    .block_on(async {
                        conn.client
                            .query("SELECT version()")
                            .fetch_all::<String>()
                            .await
                    })
                    .expect("Failed to execute query");
            });

            tasks.push(th);
        }

        for th in tasks {
            th.join().expect("Thread panicked");
        }
    }
}
