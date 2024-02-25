# Toke - TOML-based Command runner

Toke (TOML Make) is a simple command runner inspired by Make but uses TOML (Tom's Obvious, Minimal Language) instead of Makefiles.
This project was created for fun as an alternative to Make, because I was looking for a simple command runner/build system but didn't like the Makefile syntax.

# Installing toke

You can get toke from crates.io:

```sh
cargo install toke-runner
```

You can also manually compile it with cargo:

```sh
git clone https://git.olympuslab.net/afonso/toke
cd toke
cargo build --release
```

The executable should then be in `target/release/toke`

# How to use toke

Toke works by reading a TOML file named `tokefile.toml` (or any variation of it like `Tokefile.toml`, `tokefile`, etc.) in your project directory. This file contains definitions of variables, targets, and their respective commands.

You can also pass in a file to be used instead of the default one.

```sh
toke -f my_custom_named_toke_file
```

To run a toke target, just run `toke target_name_here`

For example:

```sh
toke build
```

Toke then reads the TOML file, resolves variables, and then executes the specified commands for the target you provide.

It also checks for dependency cycles in your targets to prevent infinite loops.

## Example Tokefile

```toml
[vars]
cc = "gcc"

[targets.build]
vars.cc = "clang"
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
      "vars": {
        "cc": "clang"
      },
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

The `build` target compiles the code with clang.

The `run` target runs the code with some arguments. It also depends on the `build` target.

# Variables

## Global variables

You can define global variables in the vars table, as seen in the above example.

## Local variables

You can specify local variables for each target. These local variables are defined under the `vars` key within each target section. If the local variable name matches a global variable, it will overwrite the global variable value for that specific target.

Here is an example:

```toml
[vars]
cc = "g++"

[target.target1]
vars.cc = "gcc"
cmd="${cc} ${cflags} main.c -o main"

[target.target2]
vars.cc = "clang"
vars.cflags = "-Wall"
cmd="${cc} ${cflags} main.c -o main"

[target.target3]
vars.cflags = "-O3"
cmd="${cc} ${cflags} main.c -o main"
```

In this example:

`target1` uses `gcc` for the `cc` variable, overriding the global value.

`target2` specifies `clang` for the `cc` variable and adds `-Wall` to `cflags`.

`target3` only sets `cflags` to `-O3`.

## Command line overrides

Additionally, you can override both global and local variables via command line arguments when invoking `toke`. Command line arguments follow the format `VARIABLE=value`. When provided, these values will overwrite any corresponding global or local variables.

Here's an example of using command line arguments:

```sh
toke build CC=gcc CFLAGS=-O2
```

In this example:

`CC=gcc` overrides the value of the `cc` variable.

`CFLAGS=-O2` overrides the value of the `cflags` variable.

These overrides allow for flexible customization.

# Contributing

Contributions are welcome! Feel free to submit issues or pull requests to help improve Toke.

# License

This project is licensed under the MIT License - see the LICENSE file for details.

Toke is a fun experiment aiming to simplify build systems using the TOML format.
Give it a try and see if it suits your project needs better than traditional build systems like Make!
