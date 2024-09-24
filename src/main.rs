#![no_std]
#![no_main]

use core::fmt::Write;
use core::num::Wrapping;

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_stm32::adc::Adc;
use embassy_stm32::usart::{Config, Uart};
use embassy_stm32::{bind_interrupts, peripherals, usart};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Delay, Duration};
use heapless::String;
use {defmt_rtt as _, panic_probe as _};

static SIGNAL_CHANNEL: Channel<ThreadModeRawMutex, SignalType, 4> = Channel::new();
static PUBLISH_CHANNEL: Channel<ThreadModeRawMutex, PublishSignalType, 4> = Channel::new();

bind_interrupts!(struct Irqs {
    USART3 => usart::InterruptHandler<peripherals::USART3>;
});

enum SignalType {
    Sine(f32),
    Square(f32),
}

enum PublishSignalType {
    Sine(f32, f32),
    Square(f32, f32),
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Hello World!");

    let p = embassy_stm32::init(Default::default());

    let config = Config::default();
    let usart = Uart::new(
        p.USART3, p.PC11, p.PC10, Irqs, p.DMA1_CH3, p.DMA1_CH1, config,
    )
    .unwrap();

    let mut delay = Delay;
    let mut adc = Adc::new(p.ADC1, &mut delay);
    let mut pin = p.PC4;

    let seed = adc.read(&mut pin);
    unwrap!(spawner.spawn(sine_generator(seed)));
    let seed = adc.read(&mut pin);
    unwrap!(spawner.spawn(square_generator(seed)));
    unwrap!(spawner.spawn(filter_data()));
    unwrap!(spawner.spawn(send_to_pc(usart)));
}

#[embassy_executor::task]
async fn sine_generator(seed: u16) {
    todo!();
}

#[embassy_executor::task]
async fn square_generator(seed: u16) {
    todo!();
}

#[embassy_executor::task]
async fn filter_data() {
    todo!();
}

#[embassy_executor::task]
async fn send_to_pc(
    mut uart: Uart<'static, peripherals::USART3, peripherals::DMA1_CH3, peripherals::DMA1_CH1>,
) {
    todo!();
}
