#![no_std]
#![no_main]

use embedded_hal::digital::PinState;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, OutputOpenDrain, Pull};
use esp_hal::main;
use log::info;

extern crate alloc;

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

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(72 * 1024);

    let mut sensor = OutputOpenDrain::new(peripherals.GPIO48, Level::High, Pull::None);
    let mut delay = Delay::new();

    info!("DHT22 sensor online");
    info!("reading...");

    loop {
        delay.delay_millis(2000);

        match read_sensor(&mut sensor, &mut delay) {
            Ok(_) => {}
            Err(e) => info!("Reading failed: {:?}", e),
        }
    }
}
