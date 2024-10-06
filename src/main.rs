#![no_std]
#![no_main]

use defmt::{info, unwrap};
use embassy_executor::Spawner;
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

bind_interrupts!(struct Irqs {
    USART3 => usart::InterruptHandler<peripherals::USART3>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    info!("Prog start!");

    let config = Config::default();
    let usart = Uart::new(
        p.USART3, p.PC11, p.PC10, Irqs, p.DMA1_CH3, p.DMA1_CH1, config,
    )
    .unwrap();

    let button = Input::new(p.PC13, Pull::Up);
    let button = ExtiInput::new(button, p.EXTI13);

    let led = Output::new(p.PA5, Level::High, Speed::Low);

    unwrap!(spawner.spawn(check_user_button(button)));
    unwrap!(spawner.spawn(send_to_pc(usart, led)));
}

#[embassy_executor::task]
async fn check_user_button(mut button: ExtiInput<'static, peripherals::PC13>) {
    loop {
            //0. waiting for someone to press the user button and cause a falling voltage edge
            button.wait_for_falling_edge().await;

            //1. place some smart the to the buffer
            let buf = String::<64>::try_from("User Button press identfied")
               .expect("problem creating heapless string from slice");

            //2. added the information to our publish queue
            //info!("new UART data to be sent!");
            PUBLISH_CHANNEL.send(buf).await
         }
}

#[embassy_executor::task]
async fn send_to_pc(mut uart: Uart<'static, peripherals::USART3, peripherals::DMA1_CH3, peripherals::DMA1_CH1>,
                    mut led: Output<'static, peripherals::PA5>) {
    loop
     {
        // 0. waiting for new items in the publish queue
        let str_to_publish = PUBLISH_CHANNEL.receive().await;

        // 1. Send-out the provided string data
        uart.write(str_to_publish.as_bytes()).await.unwrap();

        // 2. blink wiht the LED
        led.set_high();
        embassy_time::Timer::after(Duration::from_secs(1)).await;
        led.set_low();
        embassy_time::Timer::after(Duration::from_secs(1)).await;
    }
}