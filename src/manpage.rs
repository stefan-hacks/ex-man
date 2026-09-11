use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Represents a single option/flag from a manpage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManOption {
    /// The short flag (e.g., "-a", "-v")
    pub short: Option<String>,
    /// The long flag (e.g., "--all", "--verbose")
    pub long: Option<String>,
    /// Description of what the option does
    pub description: String,
    /// Whether the option takes an argument
    pub takes_arg: bool,
    /// The argument name placeholder (e.g., "FILE", "NUM")
    pub arg_name: Option<String>,
    /// The section this option appeared in (e.g., "DESCRIPTION", "OPTIONS")
    pub section: String,
}

impl ManOption {
    /// Format the flag as a display string like "-a, --all"
    pub fn display_string(&self) -> String {
        match (&self.short, &self.long) {
            (Some(s), Some(l)) => format!("{}, {}", s, l),
            (Some(s), None) => s.clone(),
            (None, Some(l)) => l.clone(),
            (None, None) => "???".to_string(),
        }
    }
}

/// Manpage parser that extracts options and flags from rendered manpages
pub struct ManpageParser;

impl ManpageParser {
    pub fn new() -> Self {
        Self
    }

    /// Find the manpage for a given tool and return rendered plain text
    pub fn find_and_read(&self, tool_name: &str) -> Result<String> {
        // Use `man` command to render the manpage to plain text via groff
        // This handles compressed manpages automatically
        let output = Command::new("sh")
            .args(["-c", &format!("man {} 2>/dev/null | cat", tool_name)])
            .output()?;

        if !output.status.success() || output.stdout.is_empty() {
            // Fallback: try to find manpage files directly
            let fallback = self.find_and_read_direct(tool_name)?;
            if !fallback.is_empty() {
                return Ok(fallback);
            }
            anyhow::bail!("No manpage found for '{}' (man command failed)", tool_name);
        }

        let content = String::from_utf8_lossy(&output.stdout).to_string();
        if content.trim().is_empty() {
            anyhow::bail!("Manpage for '{}' is empty", tool_name);
        }

        Ok(content)
    }

    /// Fallback: search man directories for .gz or uncompressed files
    fn find_and_read_direct(&self, tool_name: &str) -> Result<String> {
        let manpath = std::env::var("MANPATH").unwrap_or_else(|_| {
            "/usr/share/man:/usr/local/share/man:/opt/man:/nix/store".to_string()
        });

        for dir in manpath.split(':') {
            if dir.is_empty() {
                continue;
            }
            let dir_path = Path::new(dir);
            if !dir_path.exists() {
                continue;
            }

            for entry in walkdir::WalkDir::new(dir_path).max_depth(2) {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                let file_name = entry.file_name().to_string_lossy();
                // Match patterns like: ls.1, ls.1.gz, ls.1.bz2, ls.1.xz
                if !file_name.starts_with(&format!("{}.", tool_name)) {
                    continue;
                }

                let path = entry.path();
                let content = if file_name.ends_with(".gz") {
                    self.read_gz_file(path)?
                } else if file_name.ends_with(".bz2") {
                    self.read_bz2_file(path)?
                } else if file_name.ends_with(".xz") {
                    self.read_xz_file(path)?
                } else {
                    std::fs::read_to_string(path).unwrap_or_default()
                };

                if !content.is_empty() {
                    return Ok(content);
                }
            }
        }

        Ok(String::new())
    }

    fn read_gz_file(&self, path: &Path) -> Result<String> {
        let output = Command::new("zcat").arg(path).output()?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn read_bz2_file(&self, path: &Path) -> Result<String> {
        let output = Command::new("bzcat").arg(path).output()?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn read_xz_file(&self, path: &Path) -> Result<String> {
        let output = Command::new("xzcat").arg(path).output()?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Parse options from rendered manpage plain text
    pub fn parse_options(&self, content: &str) -> Result<Vec<ManOption>> {
        // First try the rendered plain text parser
        let options = self.parse_rendered_text(content);

        if !options.is_empty() {
            return Ok(options);
        }

        // Fallback: try to parse groff source if that's what we got
        self.parse_groff_source(content)
    }

    /// Parse options from rendered manpage plain text (most common case)
    /// Uses a simple line-by-line heuristic without complex regex to avoid backtracking
    fn parse_rendered_text(&self, text: &str) -> Vec<ManOption> {
        let mut options = Vec::new();
        let mut current_section = "UNKNOWN".to_string();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim();

            // Detect section headers (ALL CAPS, often standalone)
            if trimmed.len() > 2
                && trimmed
                    .chars()
                    .all(|c| c.is_uppercase() || c.is_whitespace() || c == '_' || c == '-')
                && trimmed.chars().any(|c| c.is_uppercase())
                && !trimmed.starts_with("-")
                && !trimmed.starts_with(".")
            {
                current_section = trimmed.to_string();
                i += 1;
                continue;
            }

            // Look for option lines: start with 2+ spaces then a dash
            // Pattern: "       -a, --all" or "       --output=FILE"
            let leading_spaces = line.len() - line.trim_start().len();
            if leading_spaces >= 2 && trimmed.starts_with('-') {
                // Try to parse this as an option line
                let parsed = self.parse_option_line(trimmed, &current_section);
                if let Some((opt, _consumed_lines)) = parsed {
                    // Collect description from following lines
                    let mut description = String::new();
                    i += 1;
                    let mut desc_lines = 0;
                    while i < lines.len() && desc_lines < 20 {
                        let desc_line = lines[i];
                        let desc_trimmed = desc_line.trim();

                        // Stop at empty line, new option, or section header
                        if desc_trimmed.is_empty() {
                            i += 1;
                            break;
                        }

                        // Check if next line is a new option (starts with dash after spaces)
                        let next_spaces = desc_line.len() - desc_line.trim_start().len();
                        if next_spaces >= 2 && desc_trimmed.starts_with('-') {
                            break;
                        }

                        // Check if next line is a section header
                        if desc_trimmed.len() > 2
                            && desc_trimmed
                                .chars()
                                .all(|c| c.is_uppercase() || c.is_whitespace() || c == '_' || c == '-')
                            && desc_trimmed.chars().any(|c| c.is_uppercase())
                            && !desc_trimmed.starts_with("-")
                        {
                            break;
                        }

                        // It's a continuation line - add to description
                        if !description.is_empty() {
                            description.push(' ');
                        }
                        description.push_str(desc_trimmed);
                        i += 1;
                        desc_lines += 1;
                    }

                    if description.is_empty() {
                        description = "No description available.".to_string();
                    }

                    // Deduplicate: check if we already have this exact option
                    let already_exists = options.iter().any(|o: &ManOption| {
                        (o.short == opt.short && o.long == opt.long)
                            || (o.short.is_some() && o.short == opt.short && opt.short.is_some())
                            || (o.long.is_some() && o.long == opt.long && opt.long.is_some())
                    });

                    if !already_exists {
                        options.push(ManOption {
                            short: opt.short,
                            long: opt.long,
                            description,
                            takes_arg: opt.takes_arg,
                            arg_name: opt.arg_name,
                            section: opt.section,
                        });
                    }
                    continue; // i already advanced
                }
            }

            i += 1;
        }

        options
    }

    /// Parse a single option line like "-a, --all", "--output=FILE", "-v"
    /// Returns the option and how many lines it consumed (0 for description lines)
    fn parse_option_line(
        &self,
        line: &str,
        section: &str,
    ) -> Option<(ManOption, usize)> {
        // Remove leading/trailing whitespace and control characters
        let line = line.trim().trim_start_matches('\u{0008}').trim_end_matches('\u{0008}');
        if line.is_empty() || !line.starts_with('-') {
            return None;
        }

        // Split by first occurrence of 2+ spaces (tab or double space) to separate flags from description
        // In rendered manpages: "-a, --all    do not ignore..." or "-a, --all" (no description on same line)
        let mut flags_part = line;
        let mut desc_part = "";

        if let Some(pos) = line.find("  ") {
            flags_part = &line[..pos];
            desc_part = line[pos..].trim();
        } else if let Some(pos) = line.find('\t') {
            flags_part = &line[..pos];
            desc_part = line[pos..].trim();
        }

        // Parse flags from flags_part
        let mut short = None;
        let mut long = None;
        let mut takes_arg = false;
        let mut arg_name = None;

        // Split by comma to get multiple flags
        let flag_entries: Vec<&str> = flags_part.split(',').map(|s| s.trim()).collect();

        for entry in &flag_entries {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }

            if entry.starts_with("--") {
                // Long option: --flag or --flag=ARG or --flag ARG
                let mut opt = entry.to_string();
                if let Some(eq_pos) = opt.find('=') {
                    takes_arg = true;
                    arg_name = Some(opt[eq_pos + 1..].trim().to_string());
                    opt = opt[..eq_pos].to_string();
                } else if let Some(sp_pos) = opt.find(' ') {
                    let after = opt[sp_pos + 1..].trim();
                    if !after.is_empty() && after.chars().next().unwrap().is_uppercase() {
                        takes_arg = true;
                        arg_name = Some(after.to_string());
                        opt = opt[..sp_pos].to_string();
                    }
                }
                long = Some(opt);
            } else if entry.starts_with('-') && entry.len() == 2 {
                // Short option: -x
                short = Some(entry[..2].to_string());
            } else if entry.starts_with('-') && entry.len() > 2 {
                // Could be -xARG or -x [ARG]
                short = Some(entry[..2].to_string());
                let after = entry[2..].trim();
                if !after.is_empty() && after.chars().next().unwrap().is_uppercase() {
                    takes_arg = true;
                    arg_name = Some(after.to_string());
                }
            }
        }

        if short.is_none() && long.is_none() {
            return None;
        }

        let description = if desc_part.is_empty() {
            "No description available.".to_string()
        } else {
            desc_part.to_string()
        };

        Some((
            ManOption {
                short,
                long,
                description,
                takes_arg,
                arg_name,
                section: section.to_string(),
            },
            0,
        ))
    }

    /// Parse options from groff source (fallback for direct file reading)
    fn parse_groff_source(&self, content: &str) -> Result<Vec<ManOption>> {
        let mut options = Vec::new();
        let mut current_section = "UNKNOWN".to_string();

        let section_re = Regex::new(r#"^\.SH\s+("?)(.+?)\1?$"#)?;
        let opt_short_re = Regex::new(r#"(-[A-Za-z0-9])"#)?;
        let opt_long_re = Regex::new(r#"(--[A-Za-z0-9][A-Za-z0-9\-]*)"#)?;
        let arg_re = Regex::new(r#"([A-Z_]{2,}|\u003c[^\u003e]+\u003e|[^\s\-]+\.\.\.)"#)?;

        for line in content.lines() {
            let trimmed = line.trim();

            if let Some(cap) = section_re.captures(trimmed) {
                current_section = cap.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                continue;
            }

            if trimmed.starts_with(".TP")
                || trimmed.starts_with(".IP")
                || trimmed.starts_with(".B")
                || trimmed.starts_with(".BR")
            {
                let opt_line = trimmed;
                let short_flags: Vec<String> = opt_short_re
                    .find_iter(opt_line)
                    .map(|m| m.as_str().to_string())
                    .collect();
                let long_flags: Vec<String> = opt_long_re
                    .find_iter(opt_line)
                    .map(|m| m.as_str().to_string())
                    .collect();

                if short_flags.is_empty() && long_flags.is_empty() {
                    continue;
                }

                let takes_arg =
                    opt_line.contains('=') || opt_line.contains(':') || opt_line.contains(' ');
                let arg_name = if takes_arg {
                    arg_re
                        .find(opt_line)
                        .map(|m| m.as_str().to_string())
                        .or_else(|| Some("ARG".to_string()))
                } else {
                    None
                };

                let description = self.extract_description(opt_line);

                if !short_flags.is_empty() || !long_flags.is_empty() {
                    options.push(ManOption {
                        short: short_flags.first().cloned(),
                        long: long_flags.first().cloned(),
                        description,
                        takes_arg,
                        arg_name,
                        section: current_section.clone(),
                    });
                }
            }
        }

        // Deduplicate
        let mut seen = HashMap::new();
        let mut deduped = Vec::new();
        for opt in options {
            let key = format!(
                "{}|{}",
                opt.short.as_deref().unwrap_or(""),
                opt.long.as_deref().unwrap_or("")
            );
            if seen.insert(key.clone(), true).is_none() {
                deduped.push(opt);
            }
        }

        Ok(deduped)
    }

    fn extract_description(&self, line: &str) -> String {
        let cleaned = line
            .replace(".TP", "")
            .replace(".IP", "")
            .replace(".B", "")
            .replace(".BR", "")
            .replace(".I", "")
            .replace(".IR", "")
            .replace(".RB", "")
            .replace(".RI", "")
            .replace(r#"" "#, " ")
            .replace(r#"" "#, " ")
            .trim()
            .to_string();

        let cleaned = regex::Regex::new(r#"\\[f\[]\w+"#)
            .unwrap()
            .replace_all(&cleaned, "")
            .to_string();

        if cleaned.is_empty() {
            "No description available.".to_string()
        } else {
            cleaned
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rendered_text() {
        let text = r#"
OPTIONS
       -a, --all
              do not ignore entries starting with .

       -A, --almost-all
              do not list implied . and ..

       --author
              with -l, print the author of each file

       -b, --escape
              print C-style escapes for nongraphic characters

       --block-size=SIZE
              with -l, scale sizes by SIZE when printing them
"#;

        let parser = ManpageParser::new();
        let opts = parser.parse_rendered_text(text);
        assert!(!opts.is_empty(), "Should have parsed some options");
        assert!(opts.iter().any(|o| o.short == Some("-a".to_string())));
        assert!(opts.iter().any(|o| o.long == Some("--all".to_string())));
        assert!(opts.iter().any(|o| o.long == Some("--block-size".to_string()) && o.takes_arg));
    }

    #[test]
    fn test_parse_option_line() {
        let parser = ManpageParser::new();
        let (opt, _) = parser.parse_option_line("-a, --all", "OPTIONS").unwrap();
        assert_eq!(opt.short, Some("-a".to_string()));
        assert_eq!(opt.long, Some("--all".to_string()));
        assert!(!opt.takes_arg);

        let (opt, _) = parser.parse_option_line("--output=FILE", "OPTIONS").unwrap();
        assert_eq!(opt.long, Some("--output".to_string()));
        assert!(opt.takes_arg);
        assert_eq!(opt.arg_name, Some("FILE".to_string()));
    }

    #[test]
    fn test_parse_groff_source() {
        let groff = r#"
.SH OPTIONS
.TP
.B -a, --all
Show all files, including hidden.
.TP
.BR -v , --verbose
Enable verbose output mode.
.TP
.BR -o \fIFILE\fR
Write output to FILE.
"#;

        let parser = ManpageParser::new();
        let opts = parser.parse_groff_source(groff).unwrap();
        assert!(!opts.is_empty());
        assert!(opts.iter().any(|o| o.short == Some("-a".to_string())));
    }
}
