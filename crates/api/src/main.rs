mod application;
mod handlers;
mod state;

#[cfg(test)]
mod tests;

use std::io;

#[actix_web::main]
async fn main() -> io::Result<()> {
    if let Err(err) = application::run().await {
        eprintln!("[api] bootstrap failed: {err}");
        return Err(io::Error::other(err.to_string()));
    }

    Ok(())
}
