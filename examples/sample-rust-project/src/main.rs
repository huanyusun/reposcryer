mod auth;
mod db;

use auth::AuthService;
use db::Database;

fn main() {
    let database = Database::new("sqlite://memory");
    let auth = AuthService::new(database);
    println!("{}", auth.login("alice"));
}
