# book structure

this file is the map of the whole book: every chapter file, what it covers in
one line, what state it is in, and how the pieces depend on each other.
chapter files themselves are empty until written.

## file tree

```
docs/
  book.md                          # pandoc metadata (title, toc, numbering)
  00-introduction.md               # course overview, scope decisions
  01-emulation/                    # section 1: intro, cheating past the transistor
    01-transistors-to-luts.md      # transistor theory -> LUTs -> why we skip real hw
    02-verilator-workflow.md       # verilator as our "fpga", c++ testbench harness pattern
  02-bringup/                      # section 2: first hardware
    01-blink-led.md                # first verilog module + testbench
    02-uart-and-mmio.md            # uart, introduces MMIO
  03-processor/                    # section 3: the cpu
    01-isa-encoding.md             # THE contract: instruction bits, locked first
    02-assembler.md                # assembler.py, arm asm encoding
    03-cpu.md                      # arm7_cpu.v, single-cycle core (may split later)
    04-bootrom.md                  # bootloader.asm, download code over serial
  04-compiler/                     # section 4: the "high" level language
    01-lexer-parser.md             # compiler.py front half, isa-independent
    02-codegen.md                  # compiler.py back half, targets the locked ISA
    03-linker.md                   # linker.py, elf32 arm output
    04-libc-malloc.md              # libc/, testable under qemu-arm
    05-ethernet-controller.md      # ethernet.v, mdio + mac, mmio design
    06-bootloader.md               # first c program, boots kernel over udp
  05-os/                           # section 5 (future)
    01-mmu.md
    02-kernel.md
    03-sd-card.md
    04-fat.md
    05-userland.md                 # init, shell, cat, ls, rm
  06-browser/                      # section 6 (future)
    01-tcp-stack.md
    02-telnetd.md
    03-dynamic-linking.md
    04-web-browser.md
```

section 7 (physical fpga board) is dropped: verilator is our fpga, no pcb, no
synthesis toolchain.

## status

| section | chapters | state |
|---------|----------|-------|
| 00-01   | intro, emulation | code done (section1/), chapters unwritten |
| 02      | bringup | code done (section2/), chapters unwritten |
| 03      | processor | code in progress (section3/), isa chapter is the blocker |
| 04      | compiler | code in progress (section4/), parser ~done, codegen stubbed |
| 05-06   | os, browser | future, no code yet |

## how it all connects

the book is ordered by dependency, not by original outline. three chains:

### language toolchain (the core chain)

```
compiler.py codegen -> assembler.py -> linker.py -> binary -> arm7_cpu.v decoder
```

the instruction encoding is the contract between all of them. that is why
03-processor/01-isa-encoding.md exists as its own chapter and comes before
the assembler and cpu chapters: lock the bits first, then everything that
emits or decodes them agrees. compiler codegen cannot be finished until the
isa is locked, so the isa chapter is the keystone of the whole book.

### boot chain

```
bootrom (asm, baked in) -> bootloader (c, over ethernet/udp) -> kernel (c)
```

### simulation toolchain

```
.v file + c++ testbench -> verilator -> executable model of the hardware
```

every verilog chapter uses this same pattern; 01-emulation/02 introduces it
once and the rest reference it.

## chapter format

each chapter = one buildable milestone. shape when written:

1. goal
2. non-obvious concepts only (audience is not a noobie)
3. code walkthrough of the tricky parts
4. build/run/test instructions
5. gotchas hit during the project

chapters reference code by path into sectionN/, never duplicate it.

## building the pdf

pandoc, one command, chapter order comes from filename sort:

```
pandoc docs/book.md docs/00-introduction.md docs/0[1-6]*/*.md \
  -o book.pdf --pdf-engine=xelatex
```

add pandoc + texlive to shell.nix when writing starts.
