extern crate regex;
extern crate toml;

use clap::Arg;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::{exit, Command};

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
                vec!["tokefile", "Tokefile", "tokefile.toml", "Tokefile.toml"]
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

    // Read the contents of the tokefile
    //Create all the possible file names
    //Iterate over the file names and try to read the file

    // Parse the tokefile into a TOML Value
    let parsed_toml = match toml::from_str::<toml::Value>(&tokefile_contents) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error parsing tokefile: {}", err);
            exit(1);
        }
    };

    // Extract variables from the tokefile
    let empty_table = toml::value::Table::new();
    let binding = parsed_toml.to_owned();
    let get = binding.get("vars");
    let vars = get.and_then(|vars| vars.as_table()).unwrap_or(&empty_table);

    // Replace variable instances in commands
    let replaced_commands = replace_variables(parsed_toml.clone(), vars);

    // Determine if there are dependency cycles in the tokefile
    detect_cycle(&parsed_toml);

    // Check if the target exists
    if parsed_toml
        .get("targets")
        .and_then(|targets| targets.get(&target).and_then(|t| t.get("cmd")))
        .is_none()
    {
        eprintln!("Target '{}' not found in tokefile", target);
        exit(1);
    }

    // Execute the target command
    run_command_caller(parsed_toml.clone(), &replaced_commands, target.to_string());
}
fn detect_cycle(parsed_toml: &toml::Value) {
    let mut visited_targets = HashSet::new();
    let empty_table = toml::value::Table::new();
    let targets = parsed_toml
        .get("targets")
        .and_then(|targets| targets.as_table())
        .unwrap_or(&empty_table);

    for (target_name, _) in targets.iter() {
        detect_cycle_recursive(&parsed_toml, target_name, &mut visited_targets);
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

fn replace_variables(parsed_toml: toml::Value, vars: &toml::value::Table) -> toml::Value {
    let replaced_commands = parsed_toml
        .get("targets")
        .map(|targets| {
            let mut replaced_targets = toml::value::Table::new();
            if let Some(targets) = targets.as_table() {
                for (target_name, target) in targets.iter() {
                    if let Some(target_table) = target.as_table() {
                        if let Some(cmd_value) = target_table.get("cmd") {
                            if let Some(cmd_str) = cmd_value.as_str() {
                                let replaced_cmd = replace_variables_in_cmd(cmd_str, vars);
                                replaced_targets
                                    .insert(target_name.clone(), toml::Value::String(replaced_cmd));
                            }
                        }
                    }
                }
            }
            replaced_targets
        })
        .unwrap_or(toml::value::Table::new());

    toml::Value::Table(replaced_commands)
}

fn replace_variables_in_cmd(cmd: &str, vars: &toml::value::Table) -> String {
    let mut replaced_cmd = cmd.to_string();

    // Regular expression to match variable instances like "${var_name}"
    let re = Regex::new(r#"\$\{([^}]+)\}"#).unwrap();

    for capture in re.captures_iter(&cmd) {
        if let Some(var_name) = capture.get(1).map(|m| m.as_str()) {
            if let Some(var_value) = vars.get(var_name).and_then(|v| v.as_str()) {
                replaced_cmd = replaced_cmd.replace(&format!("${{{}}}", var_name), var_value);
            }
        }
    }

    replaced_cmd
}

fn run_command_caller(parsed_toml: toml::Value, commands: &toml::Value, target: String) {
    //Create a hashset to keep track of visited targets
    let visited_targets = HashSet::new();
    run_command(parsed_toml, commands, target, &visited_targets);
}

fn run_command(
    parsed_toml: toml::Value,
    commands: &toml::Value,
    target: String,
    visited_targets: &HashSet<String>,
) {
    //Check if the target exists
    match commands.get(target.clone()) {
        Some(some_target) => {
            //Execute it's dependencies first, by order of appearance
            if let Some(target) = parsed_toml
                .get("targets")
                .and_then(|targets| targets.get(&target))
            {
                if let Some(target_table) = target.as_table() {
                    if let Some(dep_value) = target_table.get("deps") {
                        if let Some(dep_array) = dep_value.as_array() {
                            for dep in dep_array {
                                if let Some(dep_str) = dep.as_str() {
                                    run_command(
                                        parsed_toml.clone(),
                                        commands,
                                        dep_str.to_string(),
                                        visited_targets,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            if let Some(cmd) = some_target.as_str() {
                eprintln!("{}", cmd);
                let status = Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .status()
                    .expect("Failed to execute command");
                if !status.success() {
                    eprintln!(
                        "Command '{}' failed with exit code {:?}",
                        cmd,
                        status.code()
                    );
                    exit(1);
                }
            }
        }
        None => {
            eprintln!("Target '{}' not found in tokefile", target);
            exit(1);
        }
    }
}
