#![no_main]
#![no_std]

use esp32_wroom_rp::{gpio::EspControlPins, wifi::Wifi};
//use panic_halt as _;
use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use hal::{
    delay::Delay,
    pac::{self, interrupt, Interrupt, EXTI},
    prelude::*,
    spi::Spi,
    spi::{Mode, Phase, Polarity},
};
use stm32f0xx_hal as hal;

use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;

use core::{cell::RefCell, ops::DerefMut};

static INTERRUPT: Mutex<RefCell<Option<EXTI>>> = Mutex::new(RefCell::new(None));

const MODE: Mode = Mode {
    polarity: Polarity::IdleLow,
    phase: Phase::CaptureOnFirstTransition,
};

#[entry]
fn main() -> ! {
    defmt::println!("Program starting");
    let mut peripherals = pac::Peripherals::take().unwrap();
    let rcc = peripherals.RCC;
    rcc.apb2enr.modify(|_, w| w.syscfgen().set_bit());
    let mut rcc = rcc
        .configure()
        .sysclk(8.mhz())
        .freeze(&mut peripherals.FLASH);
    let cp = cortex_m::Peripherals::take().unwrap();

    let gpioa = peripherals.GPIOA.split(&mut rcc);
    let gpiob = peripherals.GPIOB.split(&mut rcc);
    let gpioc = peripherals.GPIOC.split(&mut rcc);
    let syscfg = peripherals.SYSCFG;
    let exti = peripherals.EXTI;

    let mut led = cortex_m::interrupt::free(|cs| gpioa.pa5.into_push_pull_output(cs));
    led.toggle().ok();
    let _user_button = cortex_m::interrupt::free(|cs| gpioc.pc13.into_pull_down_input(cs));
    let (sck, miso, mosi) = cortex_m::interrupt::free(|cs| {
        (
            gpiob.pb3.into_alternate_af0(cs),
            gpiob.pb4.into_alternate_af0(cs),
            gpiob.pb5.into_alternate_af0(cs),
        )
    });
    let spi = Spi::spi1(
        peripherals.SPI1,
        (sck, miso, mosi),
        MODE,
        8_000_000.hz(),
        &mut rcc,
    );
    let mut delay = Delay::new(cp.SYST, &rcc);
    let esp_pins = EspControlPins {
        cs: cortex_m::interrupt::free(|cs| gpioa.pa10.into_push_pull_output(cs)),
        gpio0: cortex_m::interrupt::free(|cs| gpioa.pa2.into_push_pull_output(cs)),
        resetn: cortex_m::interrupt::free(|cs| gpioa.pa8.into_push_pull_output(cs)),
        ack: cortex_m::interrupt::free(|cs| gpiob.pb10.into_floating_input(cs)),
    };

    let mut wifi = Wifi::init(spi, esp_pins, &mut delay).unwrap();
    let version = wifi.firmware_version();
    defmt::println!("{}", version);
    syscfg.exticr4.modify(|_, w| w.exti13().pc13());

    // Set interrupt request mask for line 13
    exti.imr.modify(|_, w| w.mr13().set_bit());

    // Set interrupt rising trigger for line 13
    exti.rtsr.modify(|_, w| w.tr13().set_bit());

    cortex_m::interrupt::free(move |cs| {
        *INTERRUPT.borrow(cs).borrow_mut() = Some(exti);
    });

    // Enable EXTI IRQ, set prio 1 and clear any pending IRQs
    let mut nvic = cp.NVIC;
    unsafe {
        nvic.set_priority(Interrupt::EXTI4_15, 1);
        cortex_m::peripheral::NVIC::unmask(Interrupt::EXTI4_15);
    }
    cortex_m::peripheral::NVIC::unpend(Interrupt::EXTI4_15);

    loop {}
}

#[interrupt]
fn EXTI4_15() {
    defmt::println!("INTERRUPT");
    cortex_m::interrupt::free(move |cs| {
        // Clear interrupt
        if let Some(exti) = INTERRUPT.borrow(cs).borrow_mut().deref_mut() {
            exti.pr.write(|w| w.pr13().set_bit());
        }
    });
}
