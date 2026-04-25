pub struct Database {
    connection: String,
}

impl Database {
    pub fn new(connection: &str) -> Self {
        Self {
            connection: connection.to_string(),
        }
    }

    pub fn connection_name(&self) -> &str {
        &self.connection
    }
}
