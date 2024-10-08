#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::Duration;
use {defmt_rtt as _, panic_probe as _};
use embassy_stm32::gpio::{Level, Output, Speed};


#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    info!("prog start");

    let mut led = Output::new(p.PA5, Level::High, Speed::Low);

    loop
     {
        led.set_high();
        embassy_time::Timer::after(Duration::from_secs(3)).await;
        led.set_low();
        embassy_time::Timer::after(Duration::from_secs(3)).await;
        info!("6 sec tick");
    }
}
