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
    let mut rnd = fastrand::Rng::with_seed(seed.into());
    let mut current = 0.0;
    loop {
        let noise = (rnd.f32() - 0.5) * 0.2;
        SIGNAL_CHANNEL
            .send(SignalType::Sine(libm::sinf(current) + noise))
            .await;
        embassy_time::Timer::after(Duration::from_millis(150)).await;
        current += 0.01;
    }
}

#[embassy_executor::task]
async fn square_generator(seed: u16) {
    let mut rnd = fastrand::Rng::with_seed(seed.into());
    let mut current = Wrapping(0);
    loop {
        let noise = (rnd.f32() - 0.5) * 0.2;
        if (current.0 / 50) % 2 == 0 {
            SIGNAL_CHANNEL.send(SignalType::Square(1.0 + noise)).await;
        } else {
            SIGNAL_CHANNEL.send(SignalType::Square(0.0 + noise)).await;
        }
        embassy_time::Timer::after(Duration::from_millis(50)).await;
        current += 1;
    }
}

struct Filter {
    value: f32,
}

impl Filter {
    fn new() -> Self {
        Self { value: 0.0 }
    }

    pub fn filter(&mut self, value: f32) -> f32 {
        let alpha = 0.7;
        self.value = self.value * alpha + (1.0 - alpha) * value;

        self.value
    }
}

#[embassy_executor::task]
async fn filter_data() {
    let mut sine_filter = Filter::new();
    let mut square_filter = Filter::new();
    loop {
        match SIGNAL_CHANNEL.receive().await {
            SignalType::Sine(v) => {
                PUBLISH_CHANNEL
                    .send(PublishSignalType::Sine(v, sine_filter.filter(v)))
                    .await
            }
            SignalType::Square(v) => {
                PUBLISH_CHANNEL
                    .send(PublishSignalType::Square(v, square_filter.filter(v)))
                    .await
            }
        };
    }
}

#[embassy_executor::task]
async fn send_to_pc(mut uart: Uart<'static, peripherals::USART3, peripherals::DMA1_CH3, peripherals::DMA1_CH1>) {
    loop {
        let d = PUBLISH_CHANNEL.receive().await;
        let mut buf = String::<64>::new();
        match d {
            PublishSignalType::Sine(raw, filtered) => {
                core::write!(&mut buf, "SINE,{},{}\r\n", raw, filtered).unwrap();
                uart.write(buf.as_bytes()).await.unwrap();
            }
            PublishSignalType::Square(raw, filtered) => {
                core::write!(&mut buf, "SQUARE,{},{}\r\n", raw, filtered).unwrap();
                uart.write(buf.as_bytes()).await.unwrap();
            }
        };
    }
}