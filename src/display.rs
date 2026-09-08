use crate::example::Example;
use crate::manpage::ManOption;
use colored::*;

/// Terminal display engine for rendering examples and options
pub struct DisplayEngine;

impl DisplayEngine {
    pub fn new() -> Self {
        Self
    }

    /// Display generated examples for a tool
    pub fn show_examples(
        &self,
        tool_name: &str,
        examples: &[Example],
        options: &[ManOption],
        json: bool,
    ) -> anyhow::Result<()> {
        if json {
            let output = serde_json::json!({
                "tool": tool_name,
                "options_count": options.len(),
                "examples_count": examples.len(),
                "options": options,
                "examples": examples,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }

        // Header
        println!("\n{}", format!("═ {} ═", "─".repeat(60)).cyan());
        println!(
            "  {} {} {}",
            "ex-man".bold().cyan(),
            "→".yellow(),
            tool_name.bold().white().on_cyan()
        );
        println!(
            "  {} {} options parsed, {} examples generated",
            "📊".yellow(),
            options.len().to_string().bold(),
            examples.len().to_string().bold()
        );
        println!("{}\n", format!("═ {} ═", "─".repeat(60)).cyan());

        // Option summary table
        if !options.is_empty() {
            println!("{}", "OPTIONS".bold().underline().cyan());
            println!(
                "  {:<15} {:<30} {}",
                "FLAG".bold(),
                "DESCRIPTION".bold(),
                "ARG".bold()
            );
            println!("  {}", "─".repeat(70));

            for opt in options.iter().take(20) {
                let flag_display = self.format_flag_brief(opt);
                let desc = if opt.description.len() > 35 {
                    format!("{}...", &opt.description[..32])
                } else {
                    opt.description.clone()
                };
                let arg = if opt.takes_arg {
                    opt.arg_name.as_deref().unwrap_or("ARG").yellow().to_string()
                } else {
                    "—".dimmed().to_string()
                };

                println!("  {:<15} {:<30} {}", flag_display, desc, arg);
            }

            if options.len() > 20 {
                println!(
                    "  {} ... and {} more options (use --raw to see all)",
                    "...".dimmed(),
                    options.len() - 20
                );
            }
            println!();
        }

        // Examples section
        if !examples.is_empty() {
            println!("{}", "EXAMPLES".bold().underline().green());
            println!();

            for (i, ex) in examples.iter().enumerate() {
                let num = format!("{:2}", i + 1).bold().green();
                println!("  {} {}", num, ex.description.white());
                println!(
                    "     {}",
                    format!("$ {}", ex.command_line).bold().yellow()
                );
                println!();
            }
        } else {
            println!("{}", "No examples generated.".yellow().italic());
        }

        // Footer
        println!("{}", format!("═ {} ═", "─".repeat(60)).cyan());
        println!(
            "  {} Run with {} for raw options, {} for JSON output",
            "💡".dimmed(),
            "--raw".cyan().bold(),
            "--json".cyan().bold()
        );
        println!("{}\n", format!("═ {} ═", "─".repeat(60)).cyan());

        Ok(())
    }

    /// Display raw extracted options without examples
    pub fn show_raw_options(
        &self,
        options: &[ManOption],
        json: bool,
    ) -> anyhow::Result<()> {
        if json {
            let output = serde_json::json!({
                "options": options,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }

        println!("\n{}", "RAW OPTIONS".bold().underline().cyan());
        println!(
            "  {:<5} {:<15} {:<25} {:<8} {}",
            "#".bold(),
            "SHORT".bold(),
            "LONG".bold(),
            "ARG?".bold(),
            "DESCRIPTION".bold()
        );
        println!("  {}", "─".repeat(90));

        for (i, opt) in options.iter().enumerate() {
            let short = opt.short.as_deref().unwrap_or("—").cyan();
            let long = opt.long.as_deref().unwrap_or("—").green();
            let arg = if opt.takes_arg { "YES".yellow() } else { "NO".dimmed() };
            let desc = &opt.description;

            println!(
                "  {:<5} {:<15} {:<25} {:<8} {}",
                i + 1,
                short,
                long,
                arg,
                desc
            );
        }

        println!("  {}\n", "─".repeat(90));
        Ok(())
    }

    fn format_flag_brief(&self,
        opt: &ManOption) -> String {
        match (&opt.short, &opt.long) {
            (Some(s), Some(l)) => format!("{}, {}", s.cyan(), l.green()),
            (Some(s), None) => s.cyan().to_string(),
            (None, Some(l)) => l.green().to_string(),
            (None, None) => "???".red().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_flag_brief() {
        let display = DisplayEngine::new();
        let opt = ManOption {
            short: Some("-a".to_string()),
            long: Some("--all".to_string()),
            description: "Show all".to_string(),
            takes_arg: false,
            arg_name: None,
            section: "OPTIONS".to_string(),
        };
        let _ = display.format_flag_brief(&opt);
    }
}
