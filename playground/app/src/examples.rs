// programs the playground ships with. the repo's own test programs are
// pulled in straight from section4/ so the playground always shows the
// real thing.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    C,
    Asm,
}

pub struct Example {
    pub name: &'static str,
    pub kind: Kind,
    pub text: &'static str,
    pub about: &'static str,
    // run with qemu-style semihosting (false = halt on svc like cpu_tb.cpp)
    pub semihost: bool,
    pub uart: bool,
}

pub const EXAMPLES: &[Example] = &[
    Example {
        name: "hello.c",
        kind: Kind::C,
        text: include_str!("../../examples/hello.c"),
        about: "small tour: structs, loops, recursion-free printing",
        semihost: true,
        uart: false,
    },
    Example {
        name: "test.c",
        kind: Kind::C,
        text: include_str!("../../../section4/test.c"),
        about: "section4/test.c: the compiler's feature test",
        semihost: true,
        uart: false,
    },
    Example {
        name: "vatest.c",
        kind: Kind::C,
        text: include_str!("../../../section4/vatest.c"),
        about: "section4/vatest.c: printf via varargs, #include, #define",
        semihost: true,
        uart: false,
    },
    Example {
        name: "vatlib.c",
        kind: Kind::C,
        text: include_str!("../../../section4/vatlib.c"),
        about: "included by vatest.c (no main, so it does not link alone)",
        semihost: true,
        uart: false,
    },
    Example {
        name: "cputest.c",
        kind: Kind::C,
        text: include_str!("../../../section4/cputest.c"),
        about: "section4/cputest.c: the cpu_tb.cpp checks, results at 0x100000",
        semihost: false,
        uart: false,
    },
    Example {
        name: "uart.c",
        kind: Kind::C,
        text: include_str!("../../examples/uart.c"),
        about: "mmio echo through the soc.v uart window",
        semihost: true,
        uart: true,
    },
    Example {
        name: "hello.s",
        kind: Kind::Asm,
        text: include_str!("../../examples/hello.s"),
        about: "hand-written assembly, no compiler or runtime",
        semihost: true,
        uart: false,
    },
    Example {
        name: "bootrom.asm",
        kind: Kind::Asm,
        text: include_str!("../../../section3/bootrom.asm"),
        about: "section3/bootrom.asm: serial loader, work in progress",
        semihost: false,
        uart: true,
    },
];
