// the from-the-transistor toolchain as a library: rust ports of the python
// compiler / assembler / linker plus a model of the verilog cpu, so the
// whole pipeline can run in a browser (wasm) or natively.

pub mod assembler;
pub mod compiler;
pub mod cpu;
pub mod disasm;
pub mod future;
pub mod linker;
pub mod machine;
pub mod pipeline;
pub mod py;
pub mod roadmap;
pub mod runtime;
