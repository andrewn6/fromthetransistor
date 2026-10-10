{ pkgs ? import <nixpkgs> { } }:

let
  # testing.txt calls plain `readelf`, which macos does not ship.
  # point it at the arm one instead of pulling in all of binutils,
  # which would shadow the system ld/as that verilator's c++ build needs.
  readelf = pkgs.writeShellScriptBin "readelf" ''
    exec arm-none-eabi-readelf "$@"
  '';
in
pkgs.mkShell {
  packages = with pkgs; [
    # sections 1-5: verilog simulation (verilator emits c++, built with make
    # and the c++ compiler that mkShell already provides)
    verilator
    gnumake
    gtkwave # .vcd waveform viewer

    # sections 3-4: assembler.py, compiler.py, linker.py
    python3
    uv

    # section 4+: reference arm toolchain (gcc, newlib + rdimon.specs, gdb,
    # objdump) and qemu for running elf files with semihosting
    gcc-arm-embedded
    qemu
    readelf

    # section 5: building and inspecting fat sd card images
    dosfstools
    mtools

    # section 6: poking the tcp stack and telnetd
    inetutils
    netcat
  ];

  shellHook = ''
    echo "fromthetransistor dev shell"
    echo "  verilator $(verilator --version | cut -d' ' -f2), $(arm-none-eabi-gcc -dumpversion) arm gcc, qemu $(qemu-system-arm --version | sed -n '1s/.*version \([0-9.]*\).*/\1/p')"
  '';
}
