#![no_main]
#![no_std]

//use panic_halt as _;
use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use hal::{
    delay::Delay,
    pac::{self, interrupt, Interrupt, EXTI},
    prelude::*,
};
use stm32f0xx_hal as hal;

use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;

use core::{cell::RefCell, ops::DerefMut};

static FREQUENCY_MS: Mutex<RefCell<Option<u16>>> = Mutex::new(RefCell::new(None));
static INTERRUPT: Mutex<RefCell<Option<EXTI>>> = Mutex::new(RefCell::new(None));
static INTERRUPT_COUNT: Mutex<RefCell<Option<u16>>> = Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    let mut peripherals = pac::Peripherals::take().unwrap();
    let rcc = peripherals.RCC;
    rcc.apb2enr.modify(|_, w| w.syscfgen().set_bit());
    let mut rcc = rcc
        .configure()
        .sysclk(8.mhz())
        .freeze(&mut peripherals.FLASH);
    let cp = cortex_m::Peripherals::take().unwrap();

    let gpioa = peripherals.GPIOA.split(&mut rcc);
    let gpioc = peripherals.GPIOC.split(&mut rcc);
    let syscfg = peripherals.SYSCFG;
    let exti = peripherals.EXTI;

    let mut led = cortex_m::interrupt::free(|cs| gpioa.pa5.into_push_pull_output(cs));
    let _button = cortex_m::interrupt::free(|cs| gpioc.pc13.into_pull_down_input(cs));
    let mut delay = Delay::new(cp.SYST, &rcc);
    let mut frequency: u16 = 1_000;
    syscfg.exticr4.modify(|_, w| w.exti13().pc13());

    // Set interrupt request mask for line 13
    exti.imr.modify(|_, w| w.mr13().set_bit());

    // Set interrupt rising trigger for line 13
    exti.rtsr.modify(|_, w| w.tr13().set_bit());

    cortex_m::interrupt::free(move |cs| {
        *FREQUENCY_MS.borrow(cs).borrow_mut() = Some(1_000);
        *INTERRUPT.borrow(cs).borrow_mut() = Some(exti);
        *INTERRUPT_COUNT.borrow(cs).borrow_mut() = Some(0);
    });

    // Enable EXTI IRQ, set prio 1 and clear any pending IRQs
    let mut nvic = cp.NVIC;
    unsafe {
        nvic.set_priority(Interrupt::EXTI4_15, 1);
        cortex_m::peripheral::NVIC::unmask(Interrupt::EXTI4_15);
    }
    cortex_m::peripheral::NVIC::unpend(Interrupt::EXTI4_15);

    loop {
        led.toggle().ok();
        cortex_m::interrupt::free(|cs| {
            frequency = FREQUENCY_MS.borrow(cs).borrow().unwrap();
            delay.delay_ms(frequency);
        });
    }
}

#[interrupt]
fn EXTI4_15() {
    cortex_m::interrupt::free(move |cs| {
        let freq = FREQUENCY_MS.borrow(cs).borrow().unwrap();
        let mut interrupt_count = INTERRUPT_COUNT.borrow(cs).borrow().unwrap();
        interrupt_count += 1;
        defmt::println!("Interrupt triggered {} times", interrupt_count);
        *FREQUENCY_MS.borrow(cs).borrow_mut() = Some(freq / 2);
        *INTERRUPT_COUNT.borrow(cs).borrow_mut() = Some(interrupt_count);
        // Clear interrupt
        if let Some(exti) = INTERRUPT.borrow(cs).borrow_mut().deref_mut() {
            exti.pr.write(|w| w.pr13().set_bit());
        }
    });
}
