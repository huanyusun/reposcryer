use crate::db::Database;

pub struct AuthService {
    database: Database,
}

impl AuthService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn login(&self, username: &str) -> String {
        format!("{} via {}", username, self.database.connection_name())
    }
}
