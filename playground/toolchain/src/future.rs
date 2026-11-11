// scaffolding for the parts of the course that are not built yet. each
// piece is wired into the emulator at the spot where the real thing will
// plug in, but does nothing interesting today. the roadmap view lists them.

use crate::machine::Device;
use std::fmt;

// ---------- section 5: mmu ----------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MmuFault {
    pub vaddr: u32,
    pub reason: &'static str,
}

impl fmt::Display for MmuFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mmu fault at 0x{:08x}: {}", self.vaddr, self.reason)
    }
}

// every cpu access goes through this. the arm9-ish mmu from the outline
// (page tables, tlb, permission bits) replaces IdentityMmu.
pub trait Mmu {
    fn translate(&mut self, vaddr: u32) -> Result<u32, MmuFault>;
    fn name(&self) -> &str;
}

// today's behavior: physical == virtual
pub struct IdentityMmu;

impl Mmu for IdentityMmu {
    fn translate(&mut self, vaddr: u32) -> Result<u32, MmuFault> {
        Ok(vaddr)
    }
    fn name(&self) -> &str {
        "identity (no mmu yet)"
    }
}

// shape of the planned tlb so the ui can draw it once it exists
#[derive(Clone, Debug, Default)]
pub struct TlbEntry {
    pub vpage: u32,
    pub ppage: u32,
    pub valid: bool,
    pub writable: bool,
    pub user: bool,
}

// ---------- section 4: ethernet (verilog exists in section4/ethernet.v) ----------

// register map copied from ethernet.v; behavior is a stub that records what
// the cpu writes so a bootloader can be developed against it
pub struct EthernetStub {
    pub base: u32,
    pub tx_len: u32,
    pub txbuf: Vec<u8>,
    pub frames_sent: Vec<Vec<u8>>,
}

impl EthernetStub {
    pub const BASE: u32 = 0x4000_2000;
    pub const CTRL: u32 = 0x0000;
    pub const STATUS: u32 = 0x0004;
    pub const TXLEN: u32 = 0x0008;
    pub const RXLEN: u32 = 0x000C;
    pub const MDIO_CTRL: u32 = 0x0010;
    pub const MDIO_DATA: u32 = 0x0014;
    pub const TXBUF: u32 = 0x1000;
    pub const RXBUF: u32 = 0x2000;
    pub const BUF_SIZE: u32 = 1536;

    pub fn new() -> Self {
        EthernetStub { base: Self::BASE, tx_len: 0, txbuf: vec![0; Self::BUF_SIZE as usize], frames_sent: Vec::new() }
    }
}

impl Default for EthernetStub {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for EthernetStub {
    fn name(&self) -> &str {
        "ethernet (stub)"
    }
    fn base(&self) -> u32 {
        self.base
    }
    fn size(&self) -> u32 {
        Self::RXBUF + Self::BUF_SIZE
    }
    fn read(&mut self, _off: u32, _side_effects: bool) -> u32 {
        // never busy, never receives: every register reads 0
        0
    }
    fn write(&mut self, off: u32, val: u32, byte: bool) {
        match off {
            Self::CTRL if val & 1 == 1 => {
                let n = (self.tx_len as usize).min(self.txbuf.len());
                self.frames_sent.push(self.txbuf[..n].to_vec());
            }
            Self::TXLEN => self.tx_len = val & 0x7FF,
            o if (Self::TXBUF..Self::TXBUF + Self::BUF_SIZE).contains(&o) => {
                let i = (o - Self::TXBUF) as usize;
                let bytes = val.to_le_bytes();
                let n = if byte { 1 } else { 4 };
                for (k, b) in bytes.iter().take(n).enumerate() {
                    if i + k < self.txbuf.len() {
                        self.txbuf[i + k] = *b;
                    }
                }
            }
            _ => {}
        }
    }
}

// ---------- section 5: sd card ----------

// spi-ish sd card window; the driver and fat code will talk to this
pub struct SdCardStub {
    pub base: u32,
    pub image: Vec<u8>,
}

impl SdCardStub {
    pub const BASE: u32 = 0x4000_3000;
    pub const BLOCK: usize = 512;
}

impl Device for SdCardStub {
    fn name(&self) -> &str {
        "sd card (stub)"
    }
    fn base(&self) -> u32 {
        self.base
    }
    fn size(&self) -> u32 {
        0x1000
    }
    fn read(&mut self, _off: u32, _side_effects: bool) -> u32 {
        0
    }
    fn write(&mut self, _off: u32, _val: u32, _byte: bool) {}
}

// ---------- section 5/6: the os abi ----------

// the syscalls the outline asks for. numbers are a proposal; the kernel
// (C, compiled by our compiler) decides them for real. the cpu will need
// svc mode + an exception vector before any of this can run on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syscall {
    Exit = 1,
    Fork = 2,
    Read = 3,
    Write = 4,
    Open = 5,
    Close = 6,
    Wait = 7,
    Execve = 11,
    Sleep = 12,
    Mmap = 90,
    Munmap = 91,
    Mprotect = 125,
    Bind = 200,
    Connect = 201,
    Send = 202,
    Recv = 203,
}

impl Syscall {
    pub const ALL: [Syscall; 16] = [
        Syscall::Open, Syscall::Read, Syscall::Write, Syscall::Close,
        Syscall::Fork, Syscall::Execve, Syscall::Wait, Syscall::Sleep, Syscall::Exit,
        Syscall::Mmap, Syscall::Munmap, Syscall::Mprotect,
        Syscall::Send, Syscall::Recv, Syscall::Bind, Syscall::Connect,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Syscall::Exit => "exit",
            Syscall::Fork => "fork",
            Syscall::Read => "read",
            Syscall::Write => "write",
            Syscall::Open => "open",
            Syscall::Close => "close",
            Syscall::Wait => "wait",
            Syscall::Execve => "execve",
            Syscall::Sleep => "sleep",
            Syscall::Mmap => "mmap",
            Syscall::Munmap => "munmap",
            Syscall::Mprotect => "mprotect",
            Syscall::Bind => "bind",
            Syscall::Connect => "connect",
            Syscall::Send => "send",
            Syscall::Recv => "recv",
        }
    }

    pub fn group(self) -> &'static str {
        match self {
            Syscall::Open | Syscall::Read | Syscall::Write | Syscall::Close => "files",
            Syscall::Fork | Syscall::Execve | Syscall::Wait | Syscall::Sleep | Syscall::Exit => "processes",
            Syscall::Mmap | Syscall::Munmap | Syscall::Mprotect => "memory",
            _ => "network (section 6)",
        }
    }
}
