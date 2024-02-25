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
    run_target(&parsed_toml, target.to_string());
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
    for (target_name, target_value) in targets.iter_mut() {
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

            //Replace variables in the target's wildcards value
            if let Some(wildcards_value) = target_table.get_mut("wildcards") {
                if let Some(wildcards_array) = wildcards_value.as_array_mut() {
                    for wildcard in wildcards_array {
                        //Check if the value is a string
                        //If it is, then it must be a command, so we need to check if it starts with the designated prefix
                        if let Some(wildcards_str) = wildcard.as_str() {
                            if let Some(stripped_command) = wildcards_str.strip_prefix('!') {
                                let replaced_command =
                                    replace_variables_in_cmd(stripped_command, &merged_vars);
                                // Execute shell command and capture its output
                                let output = Command::new("sh")
                                    .arg("-c")
                                    .arg(replaced_command)
                                    .output()
                                    .expect("Failed to execute shell command");
                                // Use the command output as the variable value
                                *wildcard = toml::Value::Array(
                                    String::from_utf8_lossy(&output.stdout)
                                        .trim()
                                        .split('\n')
                                        .map(|w| toml::Value::String(w.to_string()))
                                        .collect::<Vec<toml::Value>>(),
                                );
                            } else {
                                eprintln!("Invalid wildcard format on target {} : {} , it must either be a command (string starting with !) or an array", target_name,wildcards_str);
                            }
                        } else if let Some(wildcards_array) = wildcard.as_array_mut() {
                            for wildcard in wildcards_array {
                                if let Some(wildcard_str) = wildcard.as_str() {
                                    if wildcard_str.starts_with('!') {
                                        eprintln!("Invalid wildcard format on target {} : {} , it must either be a command (string starting with !) or an array", target_name,wildcard_str);
                                    } else {
                                        *wildcard = toml::Value::String(replace_variables_in_cmd(
                                            wildcard_str,
                                            &merged_vars,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
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
                // Check if the value starts with the designated prefix
                if let Some(stripped_command) = var_value.strip_prefix('!') {
                    // Execute shell command and capture its output
                    let output = Command::new("sh")
                        .arg("-c")
                        .arg(stripped_command)
                        .output()
                        .expect("Failed to execute shell command");
                    // Use the command output as the variable value
                    replaced_cmd = replaced_cmd.replace(
                        &format!("${{{}}}", var_name),
                        String::from_utf8_lossy(&output.stdout).trim(),
                    );
                } else {
                    // Replace variable with its value
                    replaced_cmd = replaced_cmd.replace(&format!("${{{}}}", var_name), var_value);
                }
            }
        }
    }

    replaced_cmd
}
fn wildcard_iterations(parsed_toml: &toml::Value, target: String) -> Vec<String> {
    // We want to loop over the target's wildcards and crate a list of all the combinations
    let empty_table = toml::value::Table::new();
    let targets = parsed_toml
        .get("targets")
        .and_then(|targets| targets.as_table())
        .unwrap_or(&empty_table);
    //Get the number of wildcards in the cmd value
    //A wildcard in cmd is represented by @@, so we want to count the number of times it appears
    let re = Regex::new(r#"@{2}"#).unwrap();
    let cmd = targets
        .get(&target)
        .and_then(|target| target.as_table())
        .and_then(|target| target.get("cmd"))
        .and_then(|cmd| cmd.as_str())
        .unwrap_or("");

    let wildcard_count = re.find_iter(cmd).count();
    //Check if the wildcard count is different that the number of wildcards in the wildcards value
    //If it is, then we have an invalid tokefile
    if let Some(target_table) = targets.get(&target) {
        if let Some(wildcards_value) = target_table.get("wildcards") {
            if let Some(wildcards_array) = wildcards_value.as_array() {
                if wildcard_count != wildcards_array.len() {
                    eprintln!("Invalid tokefile, the number of wildcards in the cmd value must be the same as the number of wildcards in the wildcards value");
                    exit(1);
                }
            }
        }
    };

    //Check if each wildcard has the same number of elements/iterations
    let num_of_iters = match targets.get(&target) {
        Some(target_table) => match target_table.get("wildcards") {
            Some(wildcards_value) => match wildcards_value.as_array() {
                Some(wildcards_array) => {
                    let first_length = wildcards_array[0].as_array().unwrap().len();
                    let all_same_length = wildcards_array
                        .iter()
                        .all(|wildcard| wildcard.as_array().unwrap().len() == first_length);
                    if !all_same_length {
                        eprintln!("Invalid tokefile, all wildcards must have the same number of elements/iterations");
                        exit(1);
                    } else {
                        first_length
                    }
                }
                _ => 0,
            },
            _ => 0,
        },
        _ => 0,
    };

    //Create a list of cmds where each index is a combination of the wildcards at that index
    let mut command_list = Vec::new();
    for i in 0..num_of_iters {
        let mut cmd = cmd.to_string();
        let wildcards = match targets
            .get(&target)
            .and_then(|target| target.as_table())
            .and_then(|target| target.get("wildcards"))
            .and_then(|wildcards| wildcards.as_array())
        {
            Some(wildcards) => wildcards,
            _ => {
                //Return an empty vector if the wildcards value is not an array
                return Vec::new();
            }
        };
        for wildcard in wildcards {
            if let Some(wildcard_array) = wildcard.as_array() {
                //Apple regex to replace the first instance of @@ with the value of the wildcard at index i
                cmd = cmd.replacen("@@", wildcard_array[i].as_str().unwrap(), 1);
            }
        }
        command_list.push(cmd);
    }
    command_list
}

fn run_target(parsed_toml: &toml::Value, target: String) {
    run_deps(parsed_toml, target.clone());
    run_wildcards_or_cmd(parsed_toml, target.clone());
}

fn run_wildcards_or_cmd(parsed_toml: &toml::Value, target: String) {
    if parsed_toml
        .clone()
        .get("targets")
        .and_then(|targets| targets.get(target.to_string()))
        .and_then(|target| target.as_table())
        .and_then(|target| target.get("wildcards"))
        .is_some()
    {
        let command_list = wildcard_iterations(parsed_toml, target.to_string());
        for cmd in command_list {
            run_command(&cmd);
        }
    } else {
        run_command(
            parsed_toml
                .clone()
                .get("targets")
                .and_then(|targets| targets.get(target))
                .and_then(|target| target.as_table())
                .and_then(|target| target.get("cmd"))
                .and_then(|cmd| cmd.as_str())
                .unwrap_or(""),
        );
    }
}

fn run_deps(parsed_toml: &toml::Value, target: String) {
    if let Some(targets) = parsed_toml
        .get("targets")
        .and_then(|targets| targets.as_table())
    {
        if let Some(target_table) = targets.get(&target) {
            if let Some(dep_value) = target_table.get("deps") {
                if let Some(dep_array) = dep_value.as_array() {
                    for dep in dep_array {
                        if let Some(dep_str) = dep.as_str() {
                            run_target(parsed_toml, dep_str.to_string());
                        }
                    }
                }
            }
        }
    }
}

fn run_command(cmd: &str) -> String {
    eprintln!("{}", cmd);
    let mut binding = Command::new("sh");
    let command = binding.arg("-c").arg(cmd);

    let status = command.status().expect("Failed to execute command");
    if !status.success() {
        eprintln!(
            "Command '{}' failed with exit code {:?}",
            cmd,
            status.code()
        );
        exit(1);
    }

    let output = command.output().expect("Failed to execute command");
    String::from_utf8_lossy(&output.stdout).to_string()
}
