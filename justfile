_default:
  @just --list

set dotenv-load
set dotenv-path := ".env"

alias r := run
[doc('use cargo to flash and run the execution loop')]
run:
  @cargo run
