# Toke - TOML-based Command runner

Toke (TOML Make) is a simple command runner inspired by Make but uses TOML (Tom's Obvious, Minimal Language) instead of Makefiles.
This project was created for fun as an alternative to Make, because I was looking for a simple command runner/build system but didn't like the Makefile syntax.

# How to Use Toke

Toke works by reading a TOML file named `tokefile.toml` (or any variation of it like `Tokefile.toml`, `tokefile`, etc.) in your project directory. This file contains definitions of variables, targets, and their respective commands.

## Example Tokefile

```toml
[vars]
cc = "gcc"

[targets.build]
cmd = "${cc} main.c -o main"

[targets.run]
cmd = "./main arg1 arg2"
deps = ["build"]
```

Because TOML has several ways of defining the same structure, here is a JSON representation of the above TOML file to make it easier to understand.

You can write your TOML file in any way you wish as long as it's structure is in the same style as the following JSON.

```json
{
  "vars": {
    "cc": "gcc"
  },
  "targets": {
    "build": {
      "cmd": "${cc} main.c -o main"
    },
    "run": {
      "cmd": "./main arg1 arg2",
      "deps": ["build"]
    }
  }
}
```

### In this example:

We define a variable `cc` which is set to `"gcc"`.

We have two targets: `build` and `run`.

The `build` target compiles the code with gcc.

The `run` target runs the code with some arguments. It also depends on the `build` target.

To run a specific target, simply pass its name as an argument when running the Toke program.

```
$ ./toke build
gcc main.c -o main
```

# How Toke Works

Toke reads the TOML file, resolves variables, and then executes the specified commands for the target you provide.

It also checks for dependency cycles in your targets to prevent infinite loops.

# Contributing

Contributions are welcome! Feel free to submit issues or pull requests to help improve Toke.

# License

This project is licensed under the MIT License - see the LICENSE file for details.

Toke is a fun experiment aiming to simplify build systems using the TOML format.
Give it a try and see if it suits your project needs better than traditional build systems like Make!
