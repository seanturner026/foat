_default:
  @just --list

alias r := run
[doc('use cargo to flash and run the execution loop')]
run:
  @cargo run
