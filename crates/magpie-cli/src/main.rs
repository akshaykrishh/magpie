use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use magpie_core::Store;

mod pack_source;
mod seed;

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

    /// Write a fixture database at one of five data-density tiers (zero,
    /// sparse, working, dense, degenerate) -- for verifying the redesign's
    /// UI actually holds up across data density, not just at whatever's on
    /// this machine right now. See crates/magpie-cli/src/seed.rs.
    ///
    /// Never touches the real database: with no --db, writes to a
    /// dedicated `seed-<tier>.db` next to the real one, never magpie.db
    /// itself. Point the desktop app at the result with, e.g.:
    ///   MAGPIE_DB_PATH=<printed path> pnpm tauri dev
    Seed {
        #[arg(long, value_enum)]
        tier: seed::Tier,
        /// Write somewhere other than the default seed-<tier>.db location.
        /// Overwriting an existing file at an explicit --db requires
        /// --force.
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },

    /// Prompt packs -- shared, git-hosted collections of templates. Named
    /// as its own subcommand group rather than reusing `add` (as sketched
    /// in the original design doc) since that name was already taken by
    /// plain-text capture -- `magpie add github:...` and `magpie add
    /// "some text I copied"` would otherwise be indistinguishable.
    #[command(subcommand)]
    Pack(PackCommand),
}

#[derive(Subcommand)]
enum PackCommand {
    /// Import (or re-import) a pack -- `github:owner/repo`, a full git URL,
    /// or a local directory/file (mainly for testing a pack before
    /// publishing it)
    Add { source: String },

    /// List imported packs
    List,

    /// List one pack's templates
    Templates { pack_id: i64 },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // `seed` is handled before the default database is ever opened -- it
    // resolves and opens its OWN path (never the real default, see
    // seed::resolve_db_path), and every other branch below opens the real
    // default unconditionally. If seed's handling lived down in the match
    // like everything else, just running `magpie seed ...` would still
    // create/touch the real magpie.db as a side effect of the `Store::open`
    // two lines below, silently undermining the "never touches the real
    // database" guarantee seed exists to provide. Matched by reference
    // (not `cli.command` by value) so `cli.command` is still available for
    // the exhaustive match below on every other branch.
    if let Command::Seed { tier, db, force } = &cli.command {
        let db_path = seed::resolve_db_path(*tier, db.clone(), *force)?;
        seed::run(*tier, &db_path)?;
        println!("seeded {db_path:?}");
        println!("point the desktop app at it with:");
        println!("  MAGPIE_DB_PATH={} pnpm tauri dev", db_path.display());
        return Ok(());
    }

    let db_path = magpie_core::default_db_path()
        .context("could not determine a data directory for this platform")?;
    let store =
        Store::open(&db_path).with_context(|| format!("failed to open database at {db_path:?}"))?;

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

        Command::Seed { .. } => unreachable!("handled above, before the default database opens"),

        Command::Pack(PackCommand::Add { source }) => {
            let (source_url, parsed) = pack_source::fetch(&source)?;
            let (pack, templates) = store.import_pack(&source_url, &parsed)?;
            let plural = if templates.len() == 1 { "" } else { "s" };
            println!(
                "imported pack #{} \"{}\" ({} prompt{plural})",
                pack.id,
                pack.name,
                templates.len()
            );
            for t in &templates {
                println!("  #{:<5} {}", t.id, t.title);
            }
        }

        Command::Pack(PackCommand::List) => {
            let packs = store.list_packs()?;
            if packs.is_empty() {
                println!("(no packs imported)");
            }
            for p in packs {
                println!("#{:<3} {}  ({})", p.id, p.name, p.source_url);
            }
        }

        Command::Pack(PackCommand::Templates { pack_id }) => {
            let templates = store.list_pack_templates(pack_id)?;
            if templates.is_empty() {
                println!("(no templates)");
            }
            for t in templates {
                println!("#{:<5} {}", t.id, t.title);
            }
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
