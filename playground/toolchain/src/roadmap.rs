// the course outline (ORIGINAL_OUTLINE.md) as data: what exists in the repo,
// what the playground can already run, and what is still scaffolding.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    // works end to end in the playground
    Done,
    // exists in the repo, partially wired or not runnable here yet
    Partial,
    // only interfaces / stubs in playground/toolchain/src/future.rs
    Scaffold,
    NotStarted,
    // the course step is about physical hardware, nothing to emulate
    OutOfScope,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Done => "runs here",
            Status::Partial => "partial",
            Status::Scaffold => "scaffold",
            Status::NotStarted => "not started",
            Status::OutOfScope => "hardware only",
        }
    }
}

pub struct Item {
    pub title: &'static str,
    pub lang: &'static str,
    pub status: Status,
    pub files: &'static [&'static str],
    pub note: &'static str,
}

pub struct Section {
    pub number: u32,
    pub title: &'static str,
    pub items: &'static [Item],
}

pub const SECTIONS: &[Section] = &[
    Section {
        number: 1,
        title: "intro: cheating our way past the transistor",
        items: &[Item {
            title: "emulation with verilator",
            lang: "verilog",
            status: Status::Partial,
            files: &["section1/simple.v", "section1/testbench.cpp"],
            note: "verilator flow lives in the repo; the playground replaces it with rust models",
        }],
    },
    Section {
        number: 2,
        title: "bringup: what language is hardware coded in?",
        items: &[
            Item {
                title: "blinking an led",
                lang: "verilog",
                status: Status::Partial,
                files: &["section2/blink_led.v"],
                note: "verilator only",
            },
            Item {
                title: "uart + mmio",
                lang: "verilog",
                status: Status::Partial,
                files: &["section2/uart.v", "section3/soc.v"],
                note: "the emulator maps soc.v's uart window at 0x40001000 (DATA +0, STATUS +4)",
            },
        ],
    },
    Section {
        number: 3,
        title: "processor: what is a processor anyway?",
        items: &[
            Item {
                title: "assembler",
                lang: "python -> rust",
                status: Status::Done,
                files: &["section3/assembler.py", "playground/toolchain/src/assembler.rs"],
                note: "byte-identical rust port, two passes, literal pools, relocations",
            },
            Item {
                title: "arm7 cpu",
                lang: "verilog -> rust",
                status: Status::Done,
                files: &["section3/arm7_cpu.v", "section3/cpu_tb.cpp", "playground/toolchain/src/cpu.rs"],
                note: "cycle-accurate model of the fetch/exec/block state machine",
            },
            Item {
                title: "bootrom",
                lang: "assembly",
                status: Status::Partial,
                files: &["section3/bootrom.asm"],
                note: "serial download loop; does not assemble yet (see the .s example)",
            },
        ],
    },
    Section {
        number: 4,
        title: "compiler: a \"high\" level language",
        items: &[
            Item {
                title: "c compiler",
                lang: "python -> rust",
                status: Status::Done,
                files: &["section4/compiler.py", "playground/toolchain/src/compiler/"],
                note: "structs, typedef, sizeof, varargs, #include/#define, unsigned",
            },
            Item {
                title: "linker",
                lang: "python -> rust",
                status: Status::Done,
                files: &["section4/linker.py", "playground/toolchain/src/linker.rs"],
                note: "elf32 arm, single PT_LOAD, ABS32 + CALL relocations",
            },
            Item {
                title: "libc + malloc",
                lang: "c",
                status: Status::Partial,
                files: &["section4/libc/", "section4/vatlib.c"],
                note: "libc/ needs #ifndef and casts the compiler does not have yet; vatlib.c runs",
            },
            Item {
                title: "ethernet controller",
                lang: "verilog",
                status: Status::Scaffold,
                files: &["section4/ethernet.v", "playground/toolchain/src/future.rs"],
                note: "verilog + testbench exist; emulator has a register-map stub at 0x40002000",
            },
            Item {
                title: "bootloader",
                lang: "c",
                status: Status::NotStarted,
                files: &["section4/bootloader.c"],
                note: "boot the kernel over udp via the ethernet window",
            },
        ],
    },
    Section {
        number: 5,
        title: "operating system: software we take for granted",
        items: &[
            Item {
                title: "mmu",
                lang: "verilog",
                status: Status::Scaffold,
                files: &["playground/toolchain/src/future.rs"],
                note: "every bus access already goes through the Mmu trait (identity today)",
            },
            Item {
                title: "operating system",
                lang: "c",
                status: Status::Scaffold,
                files: &["playground/toolchain/src/future.rs"],
                note: "planned syscall abi listed below; cpu needs svc mode + vectors first",
            },
            Item {
                title: "sd card",
                lang: "verilog",
                status: Status::Scaffold,
                files: &["playground/toolchain/src/future.rs"],
                note: "device stub at 0x40003000",
            },
            Item { title: "fat", lang: "c", status: Status::NotStarted, files: &[], note: "" },
            Item {
                title: "init, shell, download, cat, ls, rm",
                lang: "c",
                status: Status::NotStarted,
                files: &[],
                note: "",
            },
        ],
    },
    Section {
        number: 6,
        title: "browser: coming online",
        items: &[
            Item { title: "tcp stack", lang: "c", status: Status::NotStarted, files: &[], note: "" },
            Item { title: "telnetd", lang: "c", status: Status::NotStarted, files: &[], note: "" },
            Item {
                title: "dynamic linking",
                lang: "c",
                status: Status::NotStarted,
                files: &[],
                note: "needs linker changes",
            },
            Item { title: "text web browser", lang: "c", status: Status::NotStarted, files: &[], note: "" },
        ],
    },
    Section {
        number: 7,
        title: "physical: running on real hardware",
        items: &[
            Item { title: "talking to an fpga", lang: "c", status: Status::OutOfScope, files: &[], note: "" },
            Item { title: "building an fpga board", lang: "", status: Status::OutOfScope, files: &[], note: "" },
            Item { title: "bringup", lang: "", status: Status::OutOfScope, files: &[], note: "" },
        ],
    },
];
