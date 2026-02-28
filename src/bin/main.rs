#![no_std]
#![no_main]

use embedded_hal::digital::PinState;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, OutputOpenDrain, Pull};
use esp_hal::main;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::wifi::{
    AuthMethod, ClientConfiguration, Configuration, WifiController, WifiDevice, WifiStaDevice,
};
use esp_wifi::{init, EspWifiController};
use log::info;
use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::socket::dhcpv4;
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpCidr, Ipv4Address};
use static_cell::StaticCell;

extern crate alloc;

// --- WiFi credentials ---
const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

#[derive(Debug)]
enum DhtError {
    Timeout,
    ChecksumError,
}

// DHT22 Sequence (each reading):
//
// MCU sends start signal (18ms low, then high)
// Sensor acknowledges (80μs low, then 80μs high)
// Sensor sends 40 bits of data (5 bytes total):
//
// Humidity high byte (8 bits)
// Humidity low byte (8 bits)
// Temperature high byte (8 bits)
// Temperature low byte (8 bits)
// Checksum byte (8 bits)
//
// Sensor goes idle until next reading
//
// Each bit transmission:
//
// 50μs low (start of bit)
// 26-28μs high = bit 0
// 70μs    high = bit 1
fn read_sensor(sensor: &mut OutputOpenDrain, delay: &mut Delay) -> Result<(), DhtError> {
    sensor.set_low();
    delay.delay_millis(18);
    sensor.set_high();
    delay.delay_micros(48);

    // Sync with sensor
    wait_for_state(&*sensor, PinState::High, delay)?;
    wait_for_state(&*sensor, PinState::Low, delay)?;

    // Start reading 40 bits (5 bytes)
    let humidity_high = read_byte(&*sensor, delay)?;
    let humidity_low = read_byte(&*sensor, delay)?;
    let temperature_high = read_byte(&*sensor, delay)?;
    let temperature_low = read_byte(&*sensor, delay)?;
    let checksum = read_byte(&*sensor, delay)?;

    // humidity
    let humidity_value = ((humidity_high as u16) << 8) | (humidity_low as u16);
    let humidity_percentage = humidity_value as f32 / 10.0;

    // temperature
    let temperature_value = ((temperature_high as u16) << 8) | (temperature_low as u16);
    let temperature_celsius = temperature_value as f32 / 10.0;

    // checksum
    let sum = humidity_high
        .wrapping_add(humidity_low)
        .wrapping_add(temperature_high)
        .wrapping_add(temperature_low);

    if sum != checksum {
        info!("Checksum error: calculated {}, received {}", sum, checksum);
        return Err(DhtError::ChecksumError);
    }

    info!("Temperature: {:.1}°C", temperature_celsius);
    info!("Humidity: {:.1}%", humidity_percentage);

    Ok(())
}

fn wait_for_state(
    sensor: &OutputOpenDrain,
    state: PinState,
    delay: &mut Delay,
) -> Result<(), DhtError> {
    for _ in 0..10_000 {
        let desired_pin_state = match state {
            PinState::Low => sensor.is_low(),
            PinState::High => sensor.is_high(),
        };

        match desired_pin_state {
            true => return Ok(()),
            false => delay.delay_micros(1),
        }
    }
    Err(DhtError::Timeout)
}

fn read_byte(sensor: &OutputOpenDrain, delay: &mut Delay) -> Result<u8, DhtError> {
    let mut byte: u8 = 0;
    for n in 0..8 {
        wait_for_state(sensor, PinState::High, delay)?;
        delay.delay_micros(30);

        let is_bit_1 = sensor.is_high();
        if is_bit_1 {
            let bit_mask = 1 << (7 - (n % 8));
            byte |= bit_mask;
        }
        wait_for_state(sensor, PinState::Low, delay)?;
    }
    Ok(byte)
}

// ─── WiFi + smoltcp ───────────────────────────────────────────────────────────
/// Block until the WiFi controller has joined the AP.
fn connect_wifi(controller: &mut WifiController<'_>, delay: &mut Delay) {
    info!("Connecting to WiFi SSID: {}", WIFI_SSID);

    let client_config = Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().unwrap(),
        password: WIFI_PASSWORD.try_into().unwrap(),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    });

    controller.set_configuration(&client_config).unwrap();
    controller.start().unwrap();

    loop {
        match controller.connect() {
            Ok(_) => break,
            Err(e) => {
                info!("WiFi connect error: {:?}, retrying...", e);
                delay.delay_millis(1000);
            }
        }
    }

    loop {
        if matches!(controller.is_connected(), Ok(true)) {
            break;
        }
        delay.delay_millis(200);
    }

    info!("WiFi connected!");
}

/// Run a smoltcp poll loop until DHCP assigns an IP address.
/// Returns the assigned IPv4 address.
fn acquire_ip(
    iface: &mut Interface,
    device: &mut WifiDevice<'_, WifiStaDevice>,
    sockets: &mut SocketSet<'_>,
    dhcp_handle: smoltcp::iface::SocketHandle,
    delay: &mut Delay,
) -> Ipv4Address {
    info!("Waiting for DHCP lease...");
    loop {
        let timestamp = Instant::from_millis(0); // monotonic tick not needed for DHCP acquire
        iface.poll(timestamp, device, sockets);

        let dhcp_socket = sockets.get_mut::<dhcpv4::Socket>(dhcp_handle);
        if let Some(dhcpv4::Event::Configured(config)) = dhcp_socket.poll() {
            info!("DHCP configured: {}", config.address);
            iface.update_ip_addrs(|addrs| {
                addrs.clear();
                addrs.push(IpCidr::Ipv4(config.address)).unwrap();
            });
            if let Some(router) = config.router {
                iface.routes_mut().add_default_ipv4_route(router).unwrap();
                info!("Default gateway: {}", router);
            }
            return config.address.address();
        }

        delay.delay_millis(10);
    }
}

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(72 * 1024);

    let mut delay = Delay::new();

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let rng = Rng::new(peripherals.RNG);
    let radio_clocks = peripherals.RADIO_CLK;

    static WIFI_CTRL: StaticCell<EspWifiController<'static>> = StaticCell::new();
    let esp_wifi_ctrl = WIFI_CTRL.init(init(timg0.timer0, rng.clone(), radio_clocks).unwrap());

    let (mut wifi_device, mut controller) =
        esp_wifi::wifi::new_with_mode(esp_wifi_ctrl, peripherals.WIFI, WifiStaDevice).unwrap();

    connect_wifi(&mut controller, &mut delay);
    let mac = wifi_device.mac_address();
    let ethernet_addr = EthernetAddress(mac);
    let iface_config = Config::new(ethernet_addr.into());
    let mut iface = Interface::new(iface_config, &mut wifi_device, Instant::from_millis(0));

    let dhcp_socket = dhcpv4::Socket::new();
    let mut socket_storage = [smoltcp::iface::SocketStorage::EMPTY; 2];
    let mut sockets = SocketSet::new(&mut socket_storage[..]);
    let dhcp_handle = sockets.add(dhcp_socket);

    let ip = acquire_ip(
        &mut iface,
        &mut wifi_device,
        &mut sockets,
        dhcp_handle,
        &mut delay,
    );

    info!("Network ready. IP: {}", ip);

    let mut sensor = OutputOpenDrain::new(peripherals.GPIO48, Level::High, Pull::None);

    info!("DHT22 sensor online");
    info!("reading...");

    loop {
        delay.delay_millis(2000);

        match read_sensor(&mut sensor, &mut delay) {
            Ok(_) => {}
            Err(e) => info!("Reading failed: {:?}", e),
        }

        // Keep the WiFi stack alive
        let timestamp = Instant::from_millis(0);
        iface.poll(timestamp, &mut wifi_device, &mut sockets);
    }
}
