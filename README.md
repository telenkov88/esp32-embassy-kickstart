# esp32s3 devkit demo project

A minimal async‑Rust starter project for the **ESP32‑S3** built on top of [Embassy](https://github.com/embassy-rs/embassy) and the `esp-idf-hal` ecosystem.

## Features

1. **Runtime Wi‑Fi configuration (AP ⇄ STA)**
   * If no credentials are compiled in, the board boots as an **Access Point** named \`esp-wifi\` at **192.168.1.1**.
   * Connect to that network and open `http://192.168.1.1` in a browser to enter your home **SSID** and **password**.
   * After reboot the device starts in **Station** mode and automatically reconnects on subsequent boots.
2. **Async Web server** with Server‑Sent Events (SSE) and a simple WebSocket echo endpoint.
3. **Async HTTP client** for outbound REST/OTA download requests.
4. **Dual‑core execution** using two Embassy executors with lock‑free channels for inter‑core messaging.
5. **On‑board NeoPixel (WS2812) driver** for status LEDs and custom effects.
6. **EKV key‑value storage** for persisting configuration and runtime state across reboots.
7. **Over‑the‑air (OTA) firmware update** via HTTP with a fallback slot for safe roll‑backs.

## Quick Start

### Prerequisites

* Rust stable with the nightly **esp32s3-unknown-none-elf** target installed
* ESP‑IDF v5.x prerequisites (`idf.py`, tool‑chain in PATH)
* `make`, [`cargo-make`](https://github.com/sagiegurari/cargo-make) and **Docker** (optional)

### Build

```bash
make deps            # one‑time setup of Rust/ESP‑IDF tooling
source "$HOME/export-esp.sh"
make build           # produces build/esp32s3/firmware.bin
```

### Flash & Monitor

Set your Wi‑Fi credentials in the environment (optional):

```bash
export SSID="MyWiFi"
export PASSWORD="SuperSecret"
```

Then flash and open the serial monitor:

```bash
make run
```

### Build inside Docker

```bash
make docker-build
```

---

> **Tip:** After the first boot look at the serial monitor to find debug logs and the device's IP address when it joins your network.
