# foat - Claude Context

## Project Summary
Rust embedded firmware for an **ESP32-S3** microcontroller. Reads temperature and humidity from a **DHT22 sensor** on GPIO48 and connects to WiFi via DHCP.

## Hardware
- **MCU**: ESP32-S3 (Xtensa dual-core, 240MHz)
- **Sensor**: DHT22 (temperature + humidity) on GPIO48 via open-drain pin

## Tech Stack
- `#![no_std]` / `#![no_main]` — bare metal, no OS
- **esp-hal 0.23.1** — hardware abstraction layer
- **esp-wifi 0.12.0** — WiFi driver
- **smoltcp 0.12.0** — TCP/IP networking stack with DHCP, DNS, TCP, UDP sockets
- **static_cell 2.1.0** — safe `&'static` references without unsafe; used for `EspWifiController`
- **esp-alloc** — heap allocator (72KB heap)
- **heapless** — stack-allocated collections
- **embedded-hal 1.0** — GPIO/peripheral traits

## Code Structure (`src/bin/main.rs`)
- `DhtError` enum: `Timeout`, `ChecksumError`
- `read_sensor()` — drives DHT22 start signal, reads 40 bits (humidity x2, temp x2, checksum)
- `wait_for_state()` — busy-waits up to 10,000µs for a pin state
- `read_byte()` — reads 8 bits from sensor using timing (>30µs high = bit 1)
- `connect_wifi()` — configures WPA2 client, starts controller, blocks until connected
- `acquire_ip()` — runs smoltcp poll loop until DHCP lease is obtained
- `main()` — init peripherals → WiFi connect → DHCP → DHT22 loop (2s interval, polls WiFi stack each iteration)

## WiFi Implementation Notes
- Credentials loaded at compile time via `env!("WIFI_SSID")` / `env!("WIFI_PASSWORD")`
- `EspWifiController` stored in a `StaticCell` to get a `&'static` ref safely (no unsafe needed)
- smoltcp needs `socket-dhcpv4` feature (separate from `proto-dhcpv4`) — both are required
- WiFi stack kept alive by calling `iface.poll()` in the main sensor loop

## Build & Flash
- Target: `xtensa-esp32s3-none-elf` (set in `.cargo/config.toml`)
- Toolchain: `esp` channel (pinned in `rust-toolchain.toml`)
- Flash & monitor: `just r` (uses `espflash flash --monitor` as cargo runner)
- WiFi credentials: set in `.env` (gitignored), loaded automatically by `just` via `set dotenv-load`

## Key Notes
- Heap is 72KB — keep allocations small
- GPIO48 is used by DHT22 — don't reassign it
- `ESP_LOG=INFO` set in `.cargo/config.toml`
- Use `StaticCell` (not `static mut`) for any other `&'static` refs needed in future
