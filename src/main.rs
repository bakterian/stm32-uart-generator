#![no_std]
#![no_main]

use core::fmt::Write;
use core::num::Wrapping;
use heapless::HistoryBuffer;

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
use fastrand;
use libm::sinf;

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
   let mut degree = 0.0;
   loop
   {
      let noise = rnd.f32();
      let sin_val = sinf(degree + noise);

      SIGNAL_CHANNEL.send(SignalType::Sine(sin_val)).await;
      embassy_time::Timer::after(Duration::from_millis(200)).await;

      degree = degree + 0.0872665; //increment by 5 degrees
   }
}

#[embassy_executor::task]
async fn square_generator(seed: u16) {
   let mut rnd = fastrand::Rng::with_seed(seed.into());
   let mut counter: u32 = 0;

   let square_high: f32 = 20.0f32;
   let square_low: f32 = -20.0f32;

   embassy_time::Timer::after(Duration::from_millis(200)).await;

   loop
   {
      let noise = rnd.f32();

      let mut square_val = 0.0f32;

      if counter >= 10 {
         square_val = square_high + noise;
      }
      else {
         square_val = square_low + noise;
      }

      SIGNAL_CHANNEL.send(SignalType::Square(square_val)).await;

      embassy_time::Timer::after(Duration::from_millis(200)).await;

      counter = counter +1;

      if counter >= 20{
         counter = 0;
      }
   }
}

#[embassy_executor::task]
async fn filter_data() {

   let mut sine_hist_buf = HistoryBuffer::<f32, 8>::new();

   let mut square_hist_buf = HistoryBuffer::<f32, 4>::new();

   loop
   {
      let new_sig  = SIGNAL_CHANNEL.receive().await;

      let pub_sig_tuple = match new_sig
      {
          SignalType::Sine(noisy_sine) =>
          {
            sine_hist_buf.write(noisy_sine);
            let filtered_sine = sine_hist_buf.as_slice().iter().sum::<f32>() / sine_hist_buf.len() as f32;

            PublishSignalType::Sine(noisy_sine, filtered_sine)
          }
          SignalType::Square(noisy_square) =>
          {
            square_hist_buf.write(noisy_square);
            let filtered_square = sine_hist_buf.as_slice().iter().sum::<f32>() / sine_hist_buf.len() as f32;
            PublishSignalType::Square(noisy_square, filtered_square)
          }
      };

      PUBLISH_CHANNEL.send(pub_sig_tuple).await;
   }
}

#[embassy_executor::task]
async fn send_to_pc(mut uart: Uart<'static, peripherals::USART3, peripherals::DMA1_CH3, peripherals::DMA1_CH1>)
{
   let mut output_buf:String<80> = String::new();
   core::write!(&mut output_buf, "SIG;DIRTY;CLEAN\r\n").unwrap();
   uart.write(output_buf.as_bytes()).await.expect("problem with UART TX");

   loop
   {
      let pub_sig = PUBLISH_CHANNEL.receive().await;

      output_buf.clear();

      match pub_sig {
      PublishSignalType::Sine(unfiltered,filtered) =>
      {
         core::write!(&mut output_buf, "SINE;{:.7};{:.7}\r\n",unfiltered, filtered).unwrap();
      },
      PublishSignalType::Square(unfiltered, filtered) =>
      {
         core::write!(&mut output_buf, "SQUARE;{:.7};{:.7}\r\n",unfiltered, filtered).unwrap();
      }
      };

      uart.write(output_buf.as_bytes()).await.expect("problem with UART TX");

   }
}
