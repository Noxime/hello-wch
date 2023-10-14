#![no_std]
#![no_main]

use core::{cmp::Ordering, time::Duration};

use panic_halt as _;
use rand::{Rng, SeedableRng};
use riscv_rt::entry;

use ch32v0::ch32v003 as pac;
use ch32v00x_hal as hal;

use hal::{
    gpio::{Floating, GpioExt, Input, Output, Pin, PinState, PullDown, PullUp, PushPull},
    prelude::*,
    rcc::AHBPrescaler,
};

enum DynamicPin<const B: char, const N: u8> {
    Float(Pin<B, N, Input<Floating>>),
    Out(Pin<B, N, Output<PushPull>>),
    None,
}

impl<const B: char, const N: u8> DynamicPin<B, N> {
    pub fn new(pin: Pin<B, N, Input<Floating>>) -> Self {
        Self::Float(pin)
    }

    pub fn set_floating(&mut self) {
        let pin = core::mem::replace(self, DynamicPin::None);

        *self = DynamicPin::Float(match pin {
            DynamicPin::Float(p) => p.into_floating_input(),
            DynamicPin::Out(p) => p.into_floating_input(),
            DynamicPin::None => unreachable!(),
        })
    }

    pub fn set_out(&mut self, state: PinState) {
        let pin = core::mem::replace(self, DynamicPin::None);

        let mut pin = match pin {
            DynamicPin::Float(p) => p.into_push_pull_output(),
            DynamicPin::Out(p) => p,
            DynamicPin::None => unreachable!(),
        };

        pin.set_state(state);

        *self = DynamicPin::Out(pin);
    }
}

#[derive(Clone, Copy)]
pub enum KeyState {
    Down,
    Released,
    Up,
    Pressed,
}

impl KeyState {
    fn active(&self) -> bool {
        match self {
            KeyState::Down => false,
            KeyState::Released => true,
            KeyState::Up => false,
            KeyState::Pressed => true,
        }
    }
}

pub struct Key<const B: char, const N: u8> {
    pin: Pin<B, N, Input<PullUp>>,
    last: bool,
    time: Duration,
}

impl<const B: char, const N: u8> Key<B, N> {
    pub fn new(pin: Pin<B, N, Input<PullUp>>, time: Duration) -> Self {
        let last = pin.is_low();
        Self { pin, last, time }
    }

    pub fn update(&mut self, time: Duration) -> KeyState {
        // Debounce for 50ms
        if self.last != self.pin.is_low() && time - self.time > Duration::from_millis(50) {
            self.last = self.pin.is_low();
            self.time = time;
            self.last
                .then_some(KeyState::Pressed)
                .unwrap_or(KeyState::Released)
        } else {
            self.last.then_some(KeyState::Down).unwrap_or(KeyState::Up)
        }
    }
}

#[entry]
fn main() -> ! {
    // Initialize peripherals
    let p = pac::Peripherals::take().unwrap();

    // Power for interrupts
    p.RCC.apb2pcenr.write(|w| w.afioen().set_bit());

    let mut rcc = p.RCC.constrain();

    // HCLK = 24m / 256 = 94khz
    rcc.config.mux = hal::rcc::ClockSrc::Hsi;
    rcc.config.ahb_pre = AHBPrescaler::NotDivided;
    let clocks = rcc.config.freeze();

    let mut delay = hal::delay::CycleDelay::new(&clocks);

    // let mut debugger = unsafe { ch32v003_debug::Debugger::steal() };
    // writeln!(&mut debugger, "Hello world").unwrap();

    // Systick initialise
    p.PFIC.stk_ctlr.write(|w| {
        w.stclk()
            .set_bit() // In sync with hclk
            .ste()
            .set_bit()
    });

    // Might not be zero if not power cycled
    let mut last_systick = p.PFIC.stk_cntl.read().bits();
    let mut duration = Duration::from_secs(last_systick as u64) / clocks.hclk().to_Hz();

    // enable GPIO power domains
    let a = p.GPIOA.split(&mut rcc);
    let c = p.GPIOC.split(&mut rcc);
    let d = p.GPIOD.split(&mut rcc);

    // Output pins
    let mut d5 = DynamicPin::new(a.pa1.into_floating_input());
    let mut d4 = DynamicPin::new(c.pc4.into_floating_input());
    let mut d3 = DynamicPin::new(c.pc2.into_floating_input());
    let mut d2 = DynamicPin::new(c.pc1.into_floating_input());

    let mut key_b = Key::new(d.pd4.into_pull_up_input(), duration);
    let mut key_a = Key::new(a.pa2.into_pull_up_input(), duration);

    // Map pins to interrupts
    p.AFIO.exticr.write(|w| {
        w.exti4() // PD4 D0
            .variant(0b11)
            .exti2() // PA2 D1
            .variant(0b00)
    });

    // Enable interrupt on falling edge
    p.EXTI.ftenr.write(|w| w.tr4().set_bit().tr2().set_bit());
    // Enable interrupts from EXTI 2 and 4
    p.EXTI.evenr.write(|w| w.mr4().set_bit().mr2().set_bit());

    // Set sleep mode to standby
    p.PWR.ctlr.write(|w| w.pdds().set_bit());
    // Enable deepsleep
    p.PFIC.sctlr.write(|w| w.sleepdeep().set_bit());

    /// Light for 10us
    fn light<const AB: char, const AN: u8, const BB: char, const BN: u8>(
        high: &mut DynamicPin<AB, AN>,
        low: &mut DynamicPin<BB, BN>,
        delay: &mut impl embedded_hal::delay::DelayUs,
        cond: bool,
    ) {
        if !cond {
            // Delay in false branch as well, to keep pulse frequency regular
            delay.delay_us(1);
            return;
        }

        high.set_out(PinState::High);
        low.set_out(PinState::Low);

        delay.delay_us(1);

        high.set_floating();
        low.set_floating();
    }

    let mut rng = rand::rngs::SmallRng::seed_from_u64(0);

    let mut a = 63;
    let mut b = 63;

    let mut ap = true;
    let mut bp = true;

    let mut brightness = 1;
    let mut idle_since = duration;

    loop {
        // Calculate deltatime
        let systick = p.PFIC.stk_cntl.read().bits();
        let ticks = systick.wrapping_sub(last_systick);
        last_systick = systick;

        // Accumulate time
        duration += Duration::from_secs(ticks as u64) / clocks.hclk().to_Hz();

        // New value for either A or B
        let new = (a + b) & 0x3F;

        let key_a = key_a.update(duration);
        let key_b = key_b.update(duration);
        match (key_a, key_b) {
            (KeyState::Released, KeyState::Up) if ap && bp => a = new, // Game A
            (KeyState::Up, KeyState::Released) if ap && bp => b = new, // Game B
            (KeyState::Down, KeyState::Released) => {
                ap = false;
                a = 63;
                b = 63;
            } // Cycle brightness
            (KeyState::Released, KeyState::Down) => {
                bp = false;
                brightness = (brightness + 1) % 3;
            } // Shuffle
            (KeyState::Up, KeyState::Up) => {
                ap = true;
                bp = true;
            }
            (_, _) => {}
        }

        if key_a.active() || key_b.active() {
            idle_since = duration;
        }

        // Enter sleep
        if duration - idle_since > Duration::from_secs(10) {
            ap = false;
            bp = false;

            // This is rather fucked up
            p.PFIC.sctlr.write(|w| {
                w.wfitowfe() // Treat WFI as WFE
                    .set_bit()
                    .setevent() // No clue why this
                    .set_bit()
            });

            p.PFIC.sctlr.write(|w| {
                w.wfitowfe() // Treat WFI as WFE
                    .set_bit()
                    .setevent() // No clue why this
                    .set_bit()
            });
            unsafe {
                // Set clock to mega low
                rcc.raw()
                    .cfgr0
                    .write(|w| w.hpre().variant(AHBPrescaler::Div256 as u8));

                // Not sure why this has to be twice
                riscv::asm::wfi();
                riscv::asm::wfi();

                // Awake now :)

                // Set clock to regular
                rcc.raw()
                    .cfgr0
                    .write(|w| w.hpre().variant(AHBPrescaler::NotDivided as u8));
            }

            continue;
        }

        if a == 63 && b == 63 {
            for i in 0..10 {
                for _ in 0..100 {
                    light(&mut d2, &mut d3, &mut delay, i & 1 != 0);
                    light(&mut d3, &mut d2, &mut delay, i & 1 != 0);

                    light(&mut d3, &mut d4, &mut delay, i & 1 != 0);
                    light(&mut d4, &mut d3, &mut delay, i & 1 != 0);

                    light(&mut d4, &mut d5, &mut delay, i & 1 != 0);
                    light(&mut d5, &mut d4, &mut delay, i & 1 != 0);

                    light(&mut d2, &mut d4, &mut delay, i & 1 != 0);
                    light(&mut d4, &mut d2, &mut delay, i & 1 != 0);

                    light(&mut d3, &mut d5, &mut delay, i & 1 != 0);
                    light(&mut d5, &mut d3, &mut delay, i & 1 != 0);

                    light(&mut d2, &mut d5, &mut delay, i & 1 != 0);
                    light(&mut d5, &mut d2, &mut delay, i & 1 != 0);

                    delay.delay_us(250u32);
                }
            }
        }

        // Shuffle on win
        while (a == 63 && b == 63) || (a & 1 == 0 && b & 1 == 0) {
            a = rng.gen_range(0..64);
            b = rng.gen_range(0..64);
        }

        // Charlieplexing, 10us each, 120us total
        light(&mut d2, &mut d3, &mut delay, a & 0b100000 != 0);
        light(&mut d3, &mut d2, &mut delay, a & 0b010000 != 0);

        light(&mut d3, &mut d4, &mut delay, a & 0b001000 != 0);
        light(&mut d4, &mut d3, &mut delay, a & 0b000100 != 0);

        light(&mut d4, &mut d5, &mut delay, a & 0b000010 != 0);
        light(&mut d5, &mut d4, &mut delay, a & 0b000001 != 0);

        light(&mut d2, &mut d4, &mut delay, b & 0b100000 != 0);
        light(&mut d4, &mut d2, &mut delay, b & 0b010000 != 0);

        light(&mut d3, &mut d5, &mut delay, b & 0b001000 != 0);
        light(&mut d5, &mut d3, &mut delay, b & 0b000100 != 0);

        light(&mut d2, &mut d5, &mut delay, b & 0b000010 != 0);
        light(&mut d5, &mut d2, &mut delay, b & 0b000001 != 0);

        delay.delay_us(match brightness {
            0 => 20_000u32,
            1 => 1_000u32,
            2 => 0_000u32,
            _ => unreachable!(),
        });
    }
}
