use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use magpie_core::Store;

#[derive(Parser)]
#[command(name = "magpie", about = "Capture queue for AI-assisted work", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Capture something into the stream
    Add { text: String },

    /// List captures -- the stream by default, or Now with --now
    List {
        #[arg(long)]
        now: bool,
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },

    /// Full-text search over the stream
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: i64,
    },

    /// Mark a capture done
    Done { id: i64 },

    /// Print Now items (optionally filtered by tag) and mark them done as
    /// they're printed -- for handing work to an agent CLI, e.g.:
    /// `magpie drain --tag refactor --limit 3 | claude -p`
    Drain {
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Export everything as JSON or Markdown
    Export {
        #[arg(long, default_value = "json")]
        format: String,
    },

    /// Run the MCP server over stdio (what an agent host actually spawns)
    ServeMcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let db_path = magpie_core::default_db_path()
        .context("could not determine a data directory for this platform")?;
    let store = Store::open(&db_path)
        .with_context(|| format!("failed to open database at {db_path:?}"))?;

    match cli.command {
        Command::Add { text } => {
            let capture = store.capture(&text, None)?;
            println!("captured #{}", capture.id);
        }

        Command::List { now, limit } => {
            let captures = if now {
                store.list_now(None)?
            } else {
                store.list_stream(None, limit, 0)?
            };
            if captures.is_empty() {
                println!("(nothing here)");
            }
            for c in captures {
                let marker = if c.queue_pos.is_some() { "*" } else { " " };
                println!("{marker} #{:<5} {}", c.id, first_line(&c.body));
            }
        }

        Command::Search { query, limit } => {
            let results = store.search(&query, limit)?;
            if results.is_empty() {
                println!("(no matches)");
            }
            for c in results {
                println!("  #{:<5} {}", c.id, first_line(&c.body));
            }
        }

        Command::Done { id } => {
            store.mark_done(id)?;
            println!("done #{id}");
        }

        Command::Drain { tag, limit } => {
            let items = match tag {
                Some(tag) => store.list_captures_by_tag(&tag)?,
                None => store.list_now(None)?,
            };
            let items = items.into_iter().take(limit.unwrap_or(usize::MAX));
            let mut printed = 0;
            for c in items {
                if printed > 0 {
                    println!("\n---\n");
                }
                print!("{}", c.body);
                store.mark_done(c.id)?;
                printed += 1;
            }
            if printed == 0 {
                eprintln!("nothing to drain");
            } else {
                println!();
            }
        }

        Command::Export { format } => match format.as_str() {
            "json" => println!("{}", store.export_json()?),
            "md" | "markdown" => println!("{}", store.export_markdown()?),
            other => anyhow::bail!("unknown export format {other:?}, expected json or md"),
        },

        Command::ServeMcp => {
            magpie_mcp::serve_stdio(Arc::new(store)).await?;
        }
    }

    Ok(())
}

fn first_line(body: &str) -> String {
    let line = body.lines().next().unwrap_or("");
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(79).collect::<String>())
    } else {
        line.to_string()
    }
}
