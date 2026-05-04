_default:
  just --list --alias-style left --list-heading ''

set quiet
set dotenv-load
set dotenv-path := ".env"

# firmware
# ─────────────────────────────────────────────────────────────────────────────

alias r := run
[doc('flash the firmware and stream the serial monitor')]
[group('firmware')]
run:
  cargo run
