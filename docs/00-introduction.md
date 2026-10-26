# Introduction

What is this!?

This is my write up & build of a well-known course outline called "fromthetransistor" from George Hotz (as you can see this is a forked repository directly from him). It is a rebuild of an entire computer stack, all the way up to a web browser. Every piece is written by hand: CPU, assembler, compiler, linker, etc.

The original outline is kept verbatim in `ORIGINAL_OUTLINE.md`, adjusted for the scope decisions this repo actually made. Where this book disagrees with the outline, and the differences are listed below.

## Scope decisions

No FPGA designs of our own, but an FPGA nonetheless. The original outline ends with designing your own FPGA board, BGA reflow and all. We skip the board design, but the Verilog is kept synthesizable so it can be dropped onto an existing board. DTD, Verilator is the primary loop: every module is compiled to C++ and driven from a testbench, so the whole course runs on any loop. My thinking is that simulation is where you debug in this course; the physical FPGA is where you can make it feel real at the end.

Slight language change. The outline says the compiler is written in Haskell. It is Python (as in `section4/compiler.py`) instead. Assembler and linker are in Python, same as the outline. Libc, bootloader and kernel are in C; hardware is Verilog. No language is introduced without a reason.

*PS: I decided to choose Python in my write/build up of this course for something as the compiler, due to the sheer size, and wanted to understand the role of compiler vs trying to juggle learning a very different language at the same time*
