use std::process::exit;

mod adapters;
mod app;
mod database;
mod ingest;
mod model;

fn main() {
    if let Err(e) = app::run() {
        println!("{e}");
        exit(1)
    }
}
