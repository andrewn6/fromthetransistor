// command line front end mirroring the python entry points, so the rust
// port can be diffed against them:
//
//   ftt cc input.c [-o out.s]          compiler.py
//   ftt as input.s output.bin          assembler.py (flat binary)
//   ftt ld out.elf [in.bin ...]        linker.py (no inputs: hello demo)
//   ftt build input.c output.elf       build.py (also writes output.bin)
//   ftt sim image.bin base [--semihost] [--cputest]
//                                      cpu_tb.cpp on the rust cpu model

use std::process::exit;
use toolchain::compiler::{self, DiskFs};
use toolchain::machine::{Machine, Status, RESULTS};
use toolchain::{assembler, linker, pipeline};

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("{}", msg);
    exit(1)
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| die(format!("{}: {}", path, e)))
}

fn write(path: &str, data: &[u8]) {
    std::fs::write(path, data).unwrap_or_else(|e| die(format!("{}: {}", path, e)));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    match cmd {
        "cc" => {
            let input = args.get(2).unwrap_or_else(|| die("usage: ftt cc input.c [-o out.s]"));
            let out = compiler::compile_text(&read(input), input, &DiskFs).unwrap_or_else(|e| die(e));
            match args.iter().position(|a| a == "-o") {
                Some(i) => write(&args[i + 1], out.as_bytes()),
                None => print!("{}", out),
            }
        }
        "as" => {
            if args.len() != 4 {
                die("usage: ftt as input.s output.bin");
            }
            let a = assembler::assemble(&read(&args[2])).unwrap_or_else(|e| die(e));
            let img = a.flat_binary().unwrap_or_else(|e| die(e));
            write(&args[3], &img);
            println!("assembled {} -> {} ({} bytes, {} symbols)", args[2], args[3], img.len(), a.symbols.len());
        }
        "ld" => {
            let out = args.get(2).map(|s| s.as_str()).unwrap_or("a.out");
            let mut ld = if args.len() <= 3 {
                linker::hello_demo().unwrap_or_else(|e| die(e))
            } else {
                let mut ld = linker::ElfLinker::default();
                for b in &args[3..] {
                    let data = std::fs::read(b).unwrap_or_else(|e| die(format!("{}: {}", b, e)));
                    ld.add_section(".text", &data, linker::SHF_ALLOC | linker::SHF_EXECINSTR);
                }
                ld.add_symbol("_start", 0, ".text", linker::STB_GLOBAL, linker::STT_FUNC).unwrap_or_else(|e| die(e));
                ld.link("_start").unwrap_or_else(|e| die(e));
                ld
            };
            let (elf, _) = ld.elf_image();
            write(out, &elf);
            println!("wrote {} ({} bytes)", out, elf.len());
        }
        "build" => {
            if args.len() != 4 {
                die("usage: ftt build input.c output.elf");
            }
            let b = pipeline::build(&read(&args[2]), &args[2], &DiskFs);
            if let Some((stage, e)) = &b.error {
                die(format!("{} failed: {}", stage.name(), e));
            }
            let elf = b.elf.as_ref().unwrap();
            write(&args[3], elf);
            println!("built {} ({} bytes)", args[3], elf.len());
            let bin = format!("{}.bin", args[3].strip_suffix(".elf").unwrap_or(&args[3]));
            write(&bin, b.flat.as_ref().unwrap());
            println!("flat {} ({} bytes), load base {:#x}", bin, b.flat.as_ref().unwrap().len(), b.base);
        }
        "sim" => {
            if args.len() < 4 {
                die("usage: ftt sim image.bin base [--semihost] [--cputest]");
            }
            let img = std::fs::read(&args[2]).unwrap_or_else(|e| die(format!("{}: {}", args[2], e)));
            let entry = u32::from_str_radix(args[3].trim_start_matches("0x"), 16).unwrap_or_else(|e| die(e));
            let mut m = Machine::new();
            m.semihosting = args.iter().any(|a| a == "--semihost");
            m.load(&img, entry).unwrap_or_else(|e| die(e));
            println!("loaded {} bytes at 0x{:08x}, entry 0x{:08x}", img.len(), entry, entry);
            m.run(20_000_000);
            print!("{}", m.console);
            match &m.status {
                Status::Halted => println!("halted on svc after {} cycles", m.cycles),
                Status::Exited(code) => println!("exited (0x{:x}) after {} cycles", code, m.cycles),
                Status::Fault(e) => die(e),
                _ => die(format!("FAIL: timeout after {} cycles (cpu never hit svc)", m.cycles)),
            }
            if args.iter().any(|a| a == "--cputest") {
                // expected values from section4/cputest.c (same table as cpu_tb.cpp)
                let expected: [u32; 18] = [
                    55, 5050, 3, 2, 3000, 34, 104105, 25, 218, 255, 240, 1024, 64, 4294967292, 8, 40, 65280, 4026531840,
                ];
                let mut fails = 0;
                for (i, want) in expected.iter().enumerate() {
                    let got = m.bus.peek32(RESULTS + 4 * i as u32).unwrap_or(0);
                    let ok = got == *want;
                    fails += !ok as u32;
                    println!("check {}: got {:10} want {:10}  {}", i, got, want, if ok { "ok" } else { "FAIL" });
                }
                if fails > 0 {
                    die(format!("FAIL: {} checks", fails));
                }
                println!("ALL {} CHECKS PASS", expected.len());
            }
        }
        _ => die("usage: ftt <cc|as|ld|build|sim> ..."),
    }
}
