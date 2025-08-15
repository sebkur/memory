# Memory

A small utility for printing memory usage information about all processes
running on the machine.

Unlike tools like top, htop, etc., this tool aggregates multiple processes with the
same name to show a user-friendly overview of all instances of the same
application.


## Usage

    cargo run [limit] [--java-by=auto|jar|main]

This will display up to \<limit> lines of output, each representing a process
or a group of processes. Processes with the same name such as multiple
chrome processes are grouped to a single line.

Example output:

    Application                          Num   Memory(MB)        %    Cum.%
    chrome                                48      9134.02   58.28%   58.28%
    java: RunForceTerm                     5      1163.06    7.42%   65.70%
    wrapper-2.0                            7       248.97    1.59%   67.29%
    Xorg                                   1       187.09    1.19%   68.49%
    xfwm4                                  1        87.15    0.56%   69.04%
    mintUpdate                             1        68.66    0.44%   69.48%
    bash                                  10        54.96    0.35%   69.83%
    python3                                2        52.61    0.34%   70.17%

## Build

To build binaries, run `cargo build --release`.

Afterwards you can run `./target/release/memory`.

## Produce continuous output, like top

To run the tool with regular updates so that it looks a bit like top/htop:

    watch -n 1 ./target/release/memory
