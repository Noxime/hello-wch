cargo +custom-rv32e build
"C:\MounRiver\MounRiver_Studio\toolchain\RISC-V Embedded GCC\bin\riscv-none-embed-objcopy.exe" target/riscv32ec-unknown-none-elf/debug/hello-wch -O binary out.hex

rem copy "out.hex" "C:\MRS_DATA\workspace\CH32V003F4P6\obj\CH32V003F4P6.hex"
rem copy "target\riscv32ec-unknown-none-elf\debug\hello-wch" "C:\MRS_DATA\workspace\CH32V003F4P6\obj\CH32V003F4P6.elf"

