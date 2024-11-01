#![no_std]
#![no_main]

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_stm32::dma::NoDma;
use embassy_stm32::usart::{Config, Uart};
use embassy_stm32::{bind_interrupts, peripherals, usart};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Duration;
use heapless::String;
use {defmt_rtt as _, panic_probe as _};
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::exti::ExtiInput;

static PUBLISH_CHANNEL: Channel<ThreadModeRawMutex,String<64>, 4> = Channel::new();

const BLINK_DURATION_MS: u64 = 400;

bind_interrupts!(struct Irqs {
    USART2 => usart::InterruptHandler<peripherals::USART2>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    info!("PROG START");

    let config = Config::default();
   //  let usart = Uart::new(
   //      p.USART3, p.PC11, p.PC10, Irqs, p.DMA1_CH3, p.DMA1_CH1, config,
   //  )
    let mut usart = Uart::new(p.USART2, p.PA3, p.PA2, Irqs, NoDma, NoDma, config).unwrap();

    let button = Input::new(p.PA0, Pull::Down);
    let button = ExtiInput::new(button, p.EXTI0);

    let mut green_led = Output::new(p.PD12, Level::Low, Speed::Low);
    let orange_led = Output::new(p.PD13, Level::Low, Speed::Low);
    let blue_led = Output::new(p.PD15, Level::Low, Speed::Low);

    green_led.set_high();
    embassy_time::Timer::after(Duration::from_millis(BLINK_DURATION_MS)).await;
    green_led.set_low();
    embassy_time::Timer::after(Duration::from_millis(BLINK_DURATION_MS)).await;


    unwrap!(usart.blocking_write(b"Hello from Embassy World!\r\n"));

    unwrap!(spawner.spawn(check_user_button(button)));
    unwrap!(spawner.spawn(send_to_pc(usart, orange_led, blue_led)));
}

#[embassy_executor::task]
async fn check_user_button(mut button: ExtiInput<'static, peripherals::PA0>) {
    loop {
            //0. waiting for someone to press the user button and cause a falling voltage edge
            button.wait_for_falling_edge().await;

            //1. place some smart the to the buffer
            let buf = String::<64>::try_from("User Button press identfied\r\n")
               .expect("problem creating heapless string from slice");

            //2. added the information to our publish queue
            //info!("new UART data to be sent!");
            PUBLISH_CHANNEL.send(buf).await
         }
}

#[embassy_executor::task]
async fn send_to_pc(mut usart: Uart<'static, peripherals::USART2>,
                    mut orange_led: Output<'static, peripherals::PD13>,
                    mut blue_led: Output<'static, peripherals::PD15>) {
    
    loop
     {
        // 0. waiting for new items in the publish queue
        let str_to_publish = PUBLISH_CHANNEL.receive().await;

        // 1. Send-out the provided string data
        unwrap!(usart.blocking_write(&str_to_publish.as_bytes()));
        info!("USART TX");

        // 2. blink quickly with the LEDs
        orange_led.set_high();
        embassy_time::Timer::after(Duration::from_millis(BLINK_DURATION_MS)).await;
        orange_led.set_low();
        embassy_time::Timer::after(Duration::from_millis(BLINK_DURATION_MS)).await;

        blue_led.set_high();
        embassy_time::Timer::after(Duration::from_millis(BLINK_DURATION_MS)).await;
        blue_led.set_low();
        embassy_time::Timer::after(Duration::from_millis(BLINK_DURATION_MS)).await;
     }
}
