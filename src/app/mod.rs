use std::{env, process::exit};

use crate::app::cli::{ingest, query};

pub mod cli;

pub const USAGE_MESSAGE: &str = r#"Usage: actall-llm <ingest | query>
 - ingest <document> --mode [ pdf, markdown ]
   Ingests the given document. If mode isn't specified, it will be inferred by file extension.
   This will erase the current database.
 - query <query...>
   Runs a query on the current database, returning the 3 closest matches.
"#;

pub fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("{USAGE_MESSAGE}");
        exit(255);
    }

    match args[1].to_lowercase().as_str() {
        // Ingest
        "ingest" if args.len() < 3 => {
            println!("Usage: actall-llm ingest <document> --mode [ pdf, markdown ]");
            exit(1)
        }
        "ingest" => ingest::trigger_ingest(&args[2..]),

        // Query
        "query" if args.len() < 3 => {
            println!("Missing query message.");
            exit(1)
        }
        "query" => query::trigger_query(&args[2..]),

        _ => {
            println!("{USAGE_MESSAGE}");
            exit(255);
        }
    }
}
