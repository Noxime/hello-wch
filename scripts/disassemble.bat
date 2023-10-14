call "scripts/build.bat"

"C:\MounRiver\MounRiver_Studio\toolchain\RISC-V Embedded GCC\bin\riscv-none-embed-objdump.exe" -d target/riscv32ec-unknown-none-elf/debug/hello-wch | code -
