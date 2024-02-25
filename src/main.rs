extern crate regex;
extern crate toml;

use clap::Arg;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::{exit, Command};
use toml::map::Map;

fn main() {
    //Check if the user has provided a tokefile filename
    let matches = clap::Command::new("toke")
        .version("1.0")
        .about("A simple command runner tool to execute targets in a tokefile")
        .arg(
            Arg::new("tokefile")
                .short('f')
                .long("file")
                .value_parser(clap::value_parser!(String))
                .help("Sets the path to the tokefile"),
        )
        .arg(
            Arg::new("target")
                .index(1)
                .required(true)
                .help("Sets the target to execute"),
        )
        .arg(
            Arg::new("variables")
                .value_delimiter(' ')
                .num_args(1..)
                .index(2)
                .help("User-defined variables in the format KEY=VALUE (these override variables in the tokefile)"),
        )
        .get_matches();

    // Get the target command to execute
    let target = match matches.get_one::<String>("target") {
        Some(target) => target,
        None => {
            eprintln!("No target provided");
            exit(1);
        }
    };

    let file_path = match matches.get_one::<String>("tokefile") {
        Some(file_path) => file_path.to_owned(),
        None => {
            let file_names: Vec<String> =
                ["tokefile", "Tokefile", "tokefile.toml", "Tokefile.toml"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
            let tokefile_option =
                file_names
                    .iter()
                    .find_map(|file_name| match Path::new(file_name).exists() {
                        true => Some(file_name.to_owned()),
                        false => None,
                    });
            tokefile_option.expect("No tokefile found")
        }
    };

    let tokefile_contents = match fs::read_to_string(file_path) {
        Ok(contents) => contents.to_string(),
        Err(err) => {
            eprintln!("No tokefile found: {}", err);
            exit(1);
        }
    };

    let cli_vars = match matches.get_many::<String>("variables") {
        Some(vars) => vars.collect::<Vec<_>>(),
        None => vec![],
    };
    // Split the variables into a HashMap
    let cli_vars: HashMap<String, String> = cli_vars
        .iter()
        .map(|var| {
            let parts: Vec<&str> = var.split('=').collect();
            if parts.len() != 2 {
                eprintln!("Invalid variable format: {}", var);
                exit(1);
            }
            (parts[0].to_string(), parts[1].to_string())
        })
        .collect();

    // Parse the tokefile into a TOML Value
    let mut parsed_toml = match toml::from_str::<toml::Value>(&tokefile_contents) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error parsing tokefile: {}", err);
            exit(1);
        }
    };

    // Replace variable instances in commands
    replace_variables(&mut parsed_toml, cli_vars);

    // Determine if there are dependency cycles in the tokefile
    detect_cycle(&parsed_toml);

    // Check if the target exists
    if parsed_toml
        .get("targets")
        .and_then(|targets| targets.get(target))
        .is_none()
    {
        eprintln!("Target '{}' not found in tokefile", target);
        exit(1);
    }

    // Execute the target command
    run_command(&parsed_toml, target.to_string());
}
fn detect_cycle(parsed_toml: &toml::Value) {
    let mut visited_targets = HashSet::new();
    let empty_table = toml::value::Table::new();
    let targets = parsed_toml
        .get("targets")
        .and_then(|targets| targets.as_table())
        .unwrap_or(&empty_table);

    for (target_name, _) in targets.iter() {
        detect_cycle_recursive(parsed_toml, target_name, &mut visited_targets);
    }
}

fn detect_cycle_recursive(
    parsed_toml: &toml::Value,
    target: &str,
    visited_targets: &mut HashSet<String>,
) {
    if visited_targets.contains(target) {
        eprintln!("Cycle detected: {}", target);
        exit(1);
    }

    visited_targets.insert(target.to_string());

    if let Some(target) = parsed_toml
        .get("targets")
        .and_then(|targets| targets.get(target))
    {
        if let Some(target_table) = target.as_table() {
            if let Some(dep_value) = target_table.get("deps") {
                if let Some(dep_array) = dep_value.as_array() {
                    for dep in dep_array {
                        if let Some(dep_str) = dep.as_str() {
                            detect_cycle_recursive(parsed_toml, dep_str, visited_targets);
                        }
                    }
                }
            }
        }
    }

    visited_targets.remove(target);
}

fn replace_variables(parsed_toml: &mut toml::Value, cli_vars: HashMap<String, String>) {
    // Parse global variables
    let map = toml::value::Table::new();
    let value = &parsed_toml.clone();
    let get = value.get("vars");
    let global_vars = get.and_then(|vars| vars.as_table()).unwrap_or(&map);

    // Get the targets table or return an error if it doesn't exist
    let targets = match parsed_toml
        .get_mut("targets")
        .and_then(|targets| targets.as_table_mut())
    {
        Some(targets) => targets,
        None => {
            eprintln!("No targets found in tokefile");
            exit(1);
        }
    };

    // Iterate over each target
    for (_, target_value) in targets.iter_mut() {
        if let Some(target_table) = target_value.as_table_mut() {
            // Parse local variables for the target
            let map = toml::value::Table::new();
            let local_vars = target_table
                .get("vars")
                .and_then(|vars| vars.as_table())
                .unwrap_or(&map);

            // Merge global and local variables
            let mut merged_vars = merge_vars(global_vars, local_vars);

            // Override variables if they were provided via the CLI
            for (key, value) in cli_vars.iter() {
                merged_vars.insert(key.clone(), toml::Value::String(value.clone()));
            }

            // Replace variables in the target's cmd value
            if let Some(cmd_value) = target_table.get_mut("cmd") {
                if let Some(cmd_str) = cmd_value.as_str() {
                    *cmd_value =
                        toml::Value::String(replace_variables_in_cmd(cmd_str, &merged_vars));
                }
            }
        }
    }
}

fn merge_vars(
    global_vars: &Map<String, toml::Value>,
    local_vars: &Map<String, toml::Value>,
) -> toml::value::Table {
    let mut merged_vars = global_vars.clone();
    for (key, value) in local_vars.iter() {
        merged_vars.insert(key.clone(), value.clone());
    }
    merged_vars
}

fn replace_variables_in_cmd(cmd: &str, vars: &toml::value::Table) -> String {
    let mut replaced_cmd = cmd.to_string();

    // Regular expression to match variable instances like "${var_name}"
    let re = Regex::new(r#"\$\{([^}]+)\}"#).unwrap();

    for capture in re.captures_iter(cmd) {
        if let Some(var_name) = capture.get(1).map(|m| m.as_str()) {
            if let Some(var_value) = vars.get(var_name).and_then(|v| v.as_str()) {
                replaced_cmd = replaced_cmd.replace(&format!("${{{}}}", var_name), var_value);
            }
        }
    }

    replaced_cmd
}

fn run_command(parsed_toml: &toml::Value, target: String) {
    if let Some(targets) = parsed_toml
        .get("targets")
        .and_then(|targets| targets.as_table())
    {
        if let Some(target_table) = targets.get(&target) {
            if let Some(dep_value) = target_table.get("deps") {
                if let Some(dep_array) = dep_value.as_array() {
                    for dep in dep_array {
                        if let Some(dep_str) = dep.as_str() {
                            run_command(parsed_toml, dep_str.to_string());
                        }
                    }
                }
            }
            if let Some(cmd_value) = target_table.get("cmd") {
                if let Some(cmd_str) = cmd_value.as_str() {
                    eprintln!("{}", cmd_str);
                    let status = Command::new("sh")
                        .arg("-c")
                        .arg(cmd_str)
                        .status()
                        .expect("Failed to execute command");
                    if !status.success() {
                        eprintln!(
                            "Command '{}' failed with exit code {:?}",
                            cmd_str,
                            status.code()
                        );
                        exit(1);
                    }
                }
            }
        }
    }
}
