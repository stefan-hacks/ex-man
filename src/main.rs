use clap::Parser;
use colored::*;
use std::process;

mod db;
mod display;
mod example;
mod manpage;
mod tui;

use db::Database;
use display::DisplayEngine;
use example::ExampleEngine;
use manpage::ManpageParser;

/// An intelligent manpage example generator.
/// Smarter than tldr/tealdeer - parses local manpages, extracts flags/options,
/// and generates contextual usage examples with AI-like reasoning.
#[derive(Parser, Debug)]
#[command(name = "ex-man")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// The command/tool to look up examples for
    command: String,

    /// Force re-parsing of the manpage (ignore cache)
    #[arg(short, long)]
    refresh: bool,

    /// Show only examples for specific flags (comma-separated)
    #[arg(short, long)]
    flags: Option<String>,

    /// Show raw extracted options without generating examples
    #[arg(long)]
    raw: bool,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Force plain text output instead of TUI
    #[arg(long)]
    text: bool,

    /// Search for a command by partial name
    #[arg(short, long)]
    search: bool,

    /// Show all known tools in database
    #[arg(short, long)]
    list: bool,
}

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("{} {}", "Error:".red().bold(), e);
        process::exit(1);
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let db = Database::open()?;

    if cli.list {
        let tools = db.list_known_tools()?;
        println!("{}", "Known tools in database:".cyan().bold());
        for tool in tools {
            println!("  • {}", tool.green());
        }
        return Ok(());
    }

    if cli.search {
        let matches = db.search_tools(&cli.command)?;
        if matches.is_empty() {
            println!("{}", "No matching tools found.".yellow());
        } else {
            println!("{} '{}'", "Matches for:".cyan().bold(), cli.command);
            for m in matches {
                println!("  • {}", m.green());
            }
        }
        return Ok(());
    }

    let tool_name = &cli.command;

    // Check if we have cached data
    let needs_parsing = if cli.refresh {
        true
    } else {
        !db.has_tool(tool_name)?
    };

    let options = if needs_parsing {
        println!("{} manpage for '{}'...", "Parsing".cyan().bold(), tool_name);

        let parser = ManpageParser::new();
        let manpage_content = parser.find_and_read(tool_name)?;
        let options = parser.parse_options(&manpage_content)?;

        if options.is_empty() {
            println!("{} No options found in manpage.", "Warning:".yellow().bold());
        }

        // Store in database
        db.store_tool(tool_name, &options)?;
        options
    } else {
        println!("{} cached data for '{}'...", "Loading".cyan().bold(), tool_name);
        db.load_tool(tool_name)?
    };

    if cli.raw {
        let display = DisplayEngine::new();
        display.show_raw_options(&options, cli.json)?;
        return Ok(());
    }

    // Generate examples
    let example_engine = ExampleEngine::new();
    let examples = example_engine.generate_examples(tool_name, &options)?;

    // Use TUI unless --text is specified or stdout is not a tty
    let use_tui = !cli.text && atty::is(atty::Stream::Stdout);

    if use_tui {
        // Clear the "Parsing..." / "Loading..." lines
        print!("\x1B[2K\x1B[1A\x1B[2K\r");
        tui::run_tui(tool_name, options, examples)?;
    } else {
        let display = DisplayEngine::new();
        display.show_examples(tool_name, &examples, &options, cli.json)?;
    }

    Ok(())
}
