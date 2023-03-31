#![no_std]
#![no_main]

use panic_halt as _;
use riscv_rt::entry;

#[entry]
fn main() -> ! {
    let RCC_APB2PCENR: *mut u32 = 0x4002_1018 as _;
    let GPIOC_CFGLR: *mut u32 = 0x4001_1000 as _;
    let GPIOC_OUTDR: *mut u32 = 0x4001_100C as _;

    unsafe {
        // Enable clocks to the GPIOC bank
        RCC_APB2PCENR.write_volatile(0b10000);
        // Set pin 1 to output
        GPIOC_CFGLR.write_volatile(0b0001_0000);

        loop {
            // Set pin 1 to high
            GPIOC_OUTDR.write_volatile(0b1_0);
            riscv::asm::delay(1_000_000);

            // Set pin 1 to low
            GPIOC_OUTDR.write_volatile(0b0_0);
            riscv::asm::delay(1_000_000);
        }
    }
}
