# foat

```
$ espflash board-info
Chip type:         esp32s3 (revision v0.2)
Crystal frequency: 40 MHz
Flash size:        16MB
Features:          WiFi, BLE
```

[ESP32](https://docs.arduino.cc/hardware/nano-esp32/)

[DHT-22 Datasheet](https://cdn-shop.adafruit.com/datasheets/Digital+humidity+and+temperature+sensor+AM2302.pdf)

[GPIO](https://www.youtube.com/watch?v=QxvdmzKxEeg)

## WiFi configuration

`WIFI_SSID` and `WIFI_PASSWORD` are read at **compile time** via `env!()`, so you must rebuild after changing them. The credentials live in a gitignored `.env` file at the repo root:

```sh
cp .env.template .env
# edit .env and fill in WIFI_SSID / WIFI_PASSWORD
just r
```

`just r` loads `.env` automatically (`set dotenv-load` in the `justfile`) and re-flashes the firmware.

Notes:
- Only **WPA2 Personal** is supported (hardcoded in `connect_wifi`); open networks and WPA3 will fail.
- A blank or missing SSID surfaces as `InternalError(EspErrWifiSsid)` (error `12298`) in an infinite retry loop — check that `.env` exists and is populated.
- 2.4 GHz only (the ESP32-S3 radio does not support 5 GHz).
