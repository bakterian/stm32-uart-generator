#![no_std]
#![no_main]

use core::fmt::Write;
use core::str;
use heapless::HistoryBuffer;
use core::sync::atomic::{AtomicU32, Ordering};

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
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
//use embassy_stm32::dma::NoDma;

static SIGNAL_CHANNEL: Channel<ThreadModeRawMutex, SignalType, 4> = Channel::new();
static PUBLISH_CHANNEL: Channel<ThreadModeRawMutex, PublishSignalType, 4> = Channel::new();
const SIG_GEN_DELAY: u64 = 200;
const NOISE_MAX: u32 = 1000000;
const NOISE_MIN: u32 = 1;
static NOISE_LEVEL: AtomicU32 = AtomicU32::new(NOISE_MIN);


bind_interrupts!(struct Irqs {
    USART2 => usart::InterruptHandler<peripherals::USART2>;
});

enum SignalType {
    Sine(f32),
    Square(f32),
}

enum PublishSignalType {
    Sine(f32, f32),
    Square(f32, f32),
}

fn increase_noise()
{
   let mut curr_noise_lvl = NOISE_LEVEL.load(Ordering::Relaxed);

   if curr_noise_lvl < NOISE_MAX
   {
      curr_noise_lvl = curr_noise_lvl * 10;
      NOISE_LEVEL.store(curr_noise_lvl, Ordering::Relaxed);
      info!("Increased noise level to: {}", curr_noise_lvl);
   }
}

fn decrease_noise()
{
   let mut curr_noise_lvl = NOISE_LEVEL.load(Ordering::Relaxed);

   if curr_noise_lvl > NOISE_MIN
   {
      curr_noise_lvl = curr_noise_lvl / 10;
      NOISE_LEVEL.store(curr_noise_lvl, Ordering::Relaxed);
      info!("Decreased noise level to: {}", curr_noise_lvl);
   }
}

fn get_noise_level() -> f32
{
   (NOISE_LEVEL.load(Ordering::Relaxed) as f32) * 0.0000001_f32
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Hello World!");

    let p = embassy_stm32::init(Default::default());

    let config = Config::default();

    let usart = Uart::new(p.USART2, p.PA3, p.PA2, Irqs, p.DMA1_CH6, p.DMA1_CH5, config).unwrap();
    //let usart = Uart::new(p.USART2, p.PA3, p.PA2, Irqs, NoDma, NoDma, config).unwrap();
    //let usart = Uart::new(p.USART1, p.PA10, p.PA9, Irqs, p.DMA2_CH7, p.DMA2_CH5, config).unwrap();

    let mut delay = Delay;
    let mut adc = Adc::new(p.ADC1, &mut delay);
    let mut pin = p.PC4;

    let seed = adc.read(&mut pin);
    unwrap!(spawner.spawn(sine_generator(seed)));
    unwrap!(spawner.spawn(square_generator(seed)));
    unwrap!(spawner.spawn(filter_data()));
    unwrap!(spawner.spawn(send_to_pc(usart)));
}

#[embassy_executor::task]
async fn sine_generator(seed: u16) {
   let mut rnd: fastrand::Rng = fastrand::Rng::with_seed(seed.into());
   let mut degree = 0.0;
   loop
   {
      let noise = rnd.f32() * get_noise_level();

      let sin_val = sinf(degree + noise);

      SIGNAL_CHANNEL.send(SignalType::Sine(sin_val)).await;
      embassy_time::Timer::after(Duration::from_millis(SIG_GEN_DELAY)).await;

      degree = degree + 0.0872665; //increment by 5 degrees
   }
}

#[embassy_executor::task]
async fn square_generator(seed: u16) {
   let mut rnd = fastrand::Rng::with_seed(seed.into());
   let mut counter: u32 = 0;

   let square_high: f32 = 20.0f32;
   let square_low: f32 = -20.0f32;

   embassy_time::Timer::after(Duration::from_millis(SIG_GEN_DELAY)).await;

   loop
   {
      let noise = rnd.f32() * get_noise_level();

      let square_val:f32;

      if counter >= 10 {
         square_val = square_high + noise;
      }
      else {
         square_val = square_low + noise;
      }

      SIGNAL_CHANNEL.send(SignalType::Square(square_val)).await;

      embassy_time::Timer::after(Duration::from_millis(SIG_GEN_DELAY)).await;

      counter = counter +1;

      if counter >= 20{
         counter = 0;
      }
   }
}

#[embassy_executor::task]
async fn filter_data() {

   let mut sine_hist_buf = HistoryBuffer::<f32, 4>::new();

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
async fn send_to_pc(mut uart: Uart<'static, peripherals::USART2, peripherals::DMA1_CH6, peripherals::DMA1_CH5>)
{
   let mut output_buf:String<80> = String::new();
   let mut in_buf = [0u8;1];

   core::write!(&mut output_buf, "SIG;DIRTY;CLEAN\r\n").unwrap();
   uart.write(output_buf.as_bytes()).await.expect("problem with UART TX");

   loop
   {
      let selected_future = select(PUBLISH_CHANNEL.receive(), uart.read(&mut in_buf)).await;

      match selected_future
      {
         Either::First(pub_sig) => {

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
         },

         Either::Second(res) => {

           match res {
               Ok(_) => 
               {
                  match str::from_utf8(&in_buf) 
                  {
                     Ok(v) => 
                     {
                        info!("Just received some UART data: {}", v);

                        match v
                        {
                           "+" => increase_noise(),
                           "-" => decrease_noise(),
                           _ => info!("unsupported characters received"),
                        }                     
                     },
                     Err(_) => info!("received Invalid UTF-8 sequence via UART"),
                  };
               }
               Err(e) => info!("error during UART read: {}", e),
           }
         },      
      }


   }
}
