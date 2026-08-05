use std::{env, process::exit};

use crate::app::cli::{csv, ingest, prompt, query};

pub mod cli;

pub const USAGE_MESSAGE: &str = r#"Usage: actall-llm <ingest | query | prompt>
 - ingest <document> --mode [ pdf, markdown ]
   Ingests the given document. If mode isn't specified, it will be inferred by file extension.
   This will erase the current database.
 - ingest-stackoverflow
   Ingests the stackoverflow Q/A. This is a pretty big operation!
 - query <query...>
   Runs a query on the current database, returning the 3 closest matches.
 - prompt <model> <query...>
   Sends a prompt to an Ollama model. The model must be installed with "ollama pull <model>".
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
        "ingest-stackoverflow" => csv::trigger_stackoverflow(),

        // Query
        "query" if args.len() < 3 => {
            println!("Missing query message.");
            exit(1)
        }
        "query" => query::trigger_query(&args[2..]),

        // Prompt
        "prompt" if args.len() < 4 => {
            println!("Usage: actall-llm prompt <model> <query...>");
            exit(1)
        }
        "prompt" => prompt::trigger_prompt(&args[2..]),
        _ => {
            println!("{USAGE_MESSAGE}");
            exit(255);
        }
    }
}
