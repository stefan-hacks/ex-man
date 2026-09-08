use crate::manpage::ManOption;
use anyhow::Result;
use std::collections::HashMap;

/// Represents a generated example with command line, description, and flags used
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Example {
    pub command_line: String,
    pub description: String,
    pub flags_used: Vec<String>,
}

/// Engine that generates contextual examples from parsed manpage options
pub struct ExampleEngine;

impl ExampleEngine {
    pub fn new() -> Self {
        Self
    }

    /// Generate examples for a tool based on its parsed options
    pub fn generate_examples(
        &self,
        tool_name: &str,
        options: &[ManOption],
    ) -> Result<Vec<Example>> {
        let mut examples = Vec::new();

        // Group options by type for better example generation
        let mut flag_groups: HashMap<String, Vec<&ManOption>> = HashMap::new();

        for opt in options {
            let category = self.categorize_option(opt);
            flag_groups.entry(category).or_default().push(opt);
        }

        // Generate basic single-flag examples for each option
        for opt in options {
            let example = self.generate_single_flag_example(tool_name, opt);
            if let Some(ex) = example {
                examples.push(ex);
            }
        }

        // Generate combination examples for common flag patterns
        let combo_examples = self.generate_combination_examples(tool_name, options, &flag_groups);
        examples.extend(combo_examples);

        // Generate a "most common usage" example
        if let Some(common) = self.generate_common_usage_example(tool_name, options) {
            examples.push(common);
        }

        // Deduplicate examples by command line
        let mut seen = HashMap::new();
        let mut deduped = Vec::new();
        for ex in examples {
            let key = ex.command_line.clone();
            if seen.insert(key, true).is_none() {
                deduped.push(ex);
            }
        }

        Ok(deduped)
    }

    /// Categorize an option into a semantic group
    fn categorize_option(&self,
        opt: &ManOption) -> String {
        let desc = opt.description.to_lowercase();

        if desc.contains("help") || desc.contains("usage") || desc.contains("version") {
            "meta".to_string()
        } else if desc.contains("verbose")
            || desc.contains("debug")
            || desc.contains("quiet")
            || desc.contains("silent")
        {
            "output".to_string()
        } else if desc.contains("file")
            || desc.contains("output")
            || desc.contains("input")
            || desc.contains("directory")
            || desc.contains("path")
        {
            "io".to_string()
        } else if desc.contains("recursive")
            || desc.contains("all")
            || desc.contains("force")
            || desc.contains("interactive")
        {
            "behavior".to_string()
        } else if desc.contains("format")
            || desc.contains("style")
            || desc.contains("color")
            || desc.contains("template")
        {
            "formatting".to_string()
        } else if desc.contains("number")
            || desc.contains("count")
            || desc.contains("size")
            || desc.contains("limit")
        {
            "numeric".to_string()
        } else if desc.contains("user")
            || desc.contains("group")
            || desc.contains("permission")
            || desc.contains("owner")
        {
            "permissions".to_string()
        } else if desc.contains("network")
            || desc.contains("host")
            || desc.contains("port")
            || desc.contains("connection")
        {
            "network".to_string()
        } else {
            "general".to_string()
        }
    }

    /// Generate a single-flag example
    fn generate_single_flag_example(&self, tool_name: &str, opt: &ManOption) -> Option<Example> {
        let flag = opt.long.as_ref().or_else(|| opt.short.as_ref())?;

        let mut cmd = format!("{} {}", tool_name, flag);

        // Add a realistic argument if needed
        if opt.takes_arg {
            let arg = self.generate_realistic_arg(opt);
            cmd.push(' ');
            cmd.push_str(&arg);
        }

        let desc = format!(
            "{}: {}",
            self.format_flag_display(opt),
            self.summarize_description(&opt.description)
        );

        let mut flags = Vec::new();
        if let Some(s) = &opt.short {
            flags.push(s.clone());
        }
        if let Some(l) = &opt.long {
            flags.push(l.clone());
        }

        Some(Example {
            command_line: cmd,
            description: desc,
            flags_used: flags,
        })
    }

    /// Generate combination examples (common flag patterns)
    fn generate_combination_examples(
        &self,
        tool_name: &str,
        _options: &[ManOption],
        groups: &HashMap<String, Vec<&ManOption>>,
    ) -> Vec<Example> {
        let mut examples = Vec::new();

        // Common pattern: verbose + all files
        if let Some(output) = groups.get("output") {
            if let Some(behavior) = groups.get("behavior") {
                let verbose = output
                    .iter()
                    .find(|o| o.description.to_lowercase().contains("verbose"));
                let all = behavior
                    .iter()
                    .find(|o| o.description.to_lowercase().contains("all"));

                if let (Some(v), Some(a)) = (verbose, all) {
                    let v_flag = v.long.as_ref().or(v.short.as_ref()).unwrap();
                    let a_flag = a.long.as_ref().or(a.short.as_ref()).unwrap();
                    examples.push(Example {
                        command_line: format!("{} {} {}", tool_name, v_flag, a_flag),
                        description: format!(
                            "Show all items with verbose output ({} + {})",
                            self.format_flag_display(v),
                            self.format_flag_display(a)
                        ),
                        flags_used: vec![v_flag.clone(), a_flag.clone()],
                    });
                }
            }
        }

        // Common pattern: recursive + force
        if let Some(behavior) = groups.get("behavior") {
            let recursive = behavior
                .iter()
                .find(|o| o.description.to_lowercase().contains("recursive"));
            let force = behavior
                .iter()
                .find(|o| o.description.to_lowercase().contains("force"));

            if let (Some(r), Some(f)) = (recursive, force) {
                let r_flag = r.long.as_ref().or(r.short.as_ref()).unwrap();
                let f_flag = f.long.as_ref().or(f.short.as_ref()).unwrap();
                examples.push(Example {
                    command_line: format!("{} {} {}", tool_name, r_flag, f_flag),
                    description: format!(
                        "Recursively process with force ({} + {})",
                        self.format_flag_display(r),
                        self.format_flag_display(f)
                    ),
                    flags_used: vec![r_flag.clone(), f_flag.clone()],
                });
            }
        }

        // Common pattern: output file + format
        if let Some(io) = groups.get("io") {
            if let Some(formatting) = groups.get("formatting") {
                let output_file = io.iter().find(|o| {
                    o.description.to_lowercase().contains("output")
                        || o.description.to_lowercase().contains("file")
                });
                let format_opt = formatting
                    .iter()
                    .find(|o| o.description.to_lowercase().contains("format"));

                if let (Some(o), Some(f)) = (output_file, format_opt) {
                    let o_flag = o.long.as_ref().or(o.short.as_ref()).unwrap();
                    let f_flag = f.long.as_ref().or(f.short.as_ref()).unwrap();
                    let arg = self.generate_realistic_arg(o);
                    examples.push(Example {
                        command_line: format!(
                            "{} {} {} {} {}",
                            tool_name, f_flag, "json", o_flag, arg
                        ),
                        description: format!(
                            "Export in JSON format to a file ({} + {})",
                            self.format_flag_display(f),
                            self.format_flag_display(o)
                        ),
                        flags_used: vec![f_flag.clone(), o_flag.clone()],
                    });
                }
            }
        }

        examples
    }

    /// Generate a "most common usage" example based on the most important flags
    fn generate_common_usage_example(
        &self,
        tool_name: &str,
        options: &[ManOption],
    ) -> Option<Example> {
        // Find the most "common" flags
        let mut important_flags = Vec::new();

        for opt in options.iter().take(3) {
            if let Some(flag) = opt.long.as_ref().or_else(|| opt.short.as_ref()) {
                important_flags.push((flag.clone(), opt));
            }
        }

        if important_flags.is_empty() {
            return None;
        }

        let mut cmd = tool_name.to_string();
        let mut flags_used = Vec::new();

        for (flag, opt) in &important_flags {
            cmd.push(' ');
            cmd.push_str(flag);
            flags_used.push(flag.clone());

            if opt.takes_arg {
                let arg = self.generate_realistic_arg(opt);
                cmd.push(' ');
                cmd.push_str(&arg);
            }
        }

        Some(Example {
            command_line: cmd,
            description: "Common usage: combines the most frequently used flags".to_string(),
            flags_used,
        })
    }

    /// Generate a realistic argument value based on the option's description
    fn generate_realistic_arg(&self, opt: &ManOption) -> String {
        let desc = opt.description.to_lowercase();

        if desc.contains("file")
            || desc.contains("output")
            || desc.contains("input")
            || desc.contains("path")
        {
            if desc.contains("output") || desc.contains("write") {
                "output.txt".to_string()
            } else {
                "input.txt".to_string()
            }
        } else if desc.contains("directory") || desc.contains("dir") {
            "/path/to/directory".to_string()
        } else if desc.contains("number")
            || desc.contains("count")
            || desc.contains("limit")
            || desc.contains("size")
        {
            "10".to_string()
        } else if desc.contains("host") || desc.contains("server") {
            "example.com".to_string()
        } else if desc.contains("port") {
            "8080".to_string()
        } else if desc.contains("user") || desc.contains("username") {
            "username".to_string()
        } else if desc.contains("format") || desc.contains("style") {
            "json".to_string()
        } else if desc.contains("time") || desc.contains("seconds") {
            "30".to_string()
        } else {
            opt.arg_name.clone().unwrap_or_else(|| "ARG".to_string())
        }
    }

    /// Format a flag display (e.g., "-a, --all")
    fn format_flag_display(&self, opt: &ManOption) -> String {
        match (&opt.short, &opt.long) {
            (Some(s), Some(l)) => format!("{}, {}", s, l),
            (Some(s), None) => s.clone(),
            (None, Some(l)) => l.clone(),
            (None, None) => "???".to_string(),
        }
    }

    /// Summarize a long description into a shorter phrase
    fn summarize_description(&self, desc: &str) -> String {
        let cleaned = desc.replace('\n', " ").replace("  ", " ");

        let sentences: Vec<&str> = cleaned.split('.').collect();
        let first = sentences.first().unwrap_or(&"No description");

        let summary = first.trim().to_string();
        if summary.len() > 80 {
            format!("{}...", &summary[..77])
        } else {
            summary
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_single_flag_example() {
        let engine = ExampleEngine::new();
        let opt = ManOption {
            short: Some("-v".to_string()),
            long: Some("--verbose".to_string()),
            description: "Enable verbose output mode".to_string(),
            takes_arg: false,
            arg_name: None,
            section: "OPTIONS".to_string(),
        };

        let ex = engine.generate_single_flag_example("ls", &opt).unwrap();
        assert_eq!(ex.command_line, "ls --verbose");
        assert!(ex.description.contains("verbose"));
    }

    #[test]
    fn test_generate_single_flag_with_arg() {
        let engine = ExampleEngine::new();
        let opt = ManOption {
            short: Some("-o".to_string()),
            long: Some("--output".to_string()),
            description: "Write output to FILE".to_string(),
            takes_arg: true,
            arg_name: Some("FILE".to_string()),
            section: "OPTIONS".to_string(),
        };

        let ex = engine.generate_single_flag_example("ls", &opt).unwrap();
        assert_eq!(ex.command_line, "ls --output output.txt");
    }

    #[test]
    fn test_generate_examples() {
        let engine = ExampleEngine::new();
        let options = vec![
            ManOption {
                short: Some("-a".to_string()),
                long: Some("--all".to_string()),
                description: "Show all files including hidden".to_string(),
                takes_arg: false,
                arg_name: None,
                section: "OPTIONS".to_string(),
            },
            ManOption {
                short: Some("-v".to_string()),
                long: Some("--verbose".to_string()),
                description: "Enable verbose output".to_string(),
                takes_arg: false,
                arg_name: None,
                section: "OPTIONS".to_string(),
            },
            ManOption {
                short: Some("-r".to_string()),
                long: Some("--recursive".to_string()),
                description: "Process directories recursively".to_string(),
                takes_arg: false,
                arg_name: None,
                section: "OPTIONS".to_string(),
            },
        ];

        let examples = engine.generate_examples("ls", &options).unwrap();
        assert!(!examples.is_empty());

        // Should have individual flag examples
        assert!(examples.iter().any(|e| e.command_line.contains("--all")));
        assert!(examples
            .iter()
            .any(|e| e.command_line.contains("--verbose")));
        assert!(examples
            .iter()
            .any(|e| e.command_line.contains("--recursive")));

        // Should have combination examples
        assert!(examples
            .iter()
            .any(|e| e.command_line.contains("--verbose") && e.command_line.contains("--all")));
    }
}
