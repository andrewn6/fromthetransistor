#!/usr/bin/env bash
# de10.sh - drive a de10-nano from an apple-silicon mac.
#
# quartus runs in the lima vm "quartus" (see ~/dev/QuartusForMacOS),
# openocd runs on the mac and talks to the board's usb-blaster ii.
# ~/QuartusVM on the mac is /home/user/QuartusVM in the vm, so builds
# happen there and their output lands on the mac.
#
#   de10.sh doctor             check every piece and say what's missing
#   de10.sh vm start|stop      start / stop the quartus vm
#   de10.sh install-cyclonev [file.qdz]
#                              unpack cyclone v device support into quartus
#                              (default ~/QuartusVM/cyclonev-20.1.1.720.qdz)
#   de10.sh firmware           copy blaster_6810.hex out of the vm for openocd
#   de10.sh detect             list the board's jtag chain
#   de10.sh synth [family device]
#                              synthesize the whole soc, print its size
#                              (default "Cyclone IV E" EP4CE115F29C7)
#   de10.sh compile <projdir>  compile a quartus project, then make an .svf
#   de10.sh flash <file.svf>   load an .svf into the fpga (lost on power off)
#   de10.sh blink              compile + flash the led blink smoke test
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
VM=quartus
QROOT=/home/user/intelFPGA_lite/20.1
QBIN=$QROOT/quartus/bin
SHARED_MAC="$HOME/QuartusVM"
SHARED_VM=/home/user/QuartusVM
CACHE="$HOME/.cache/de10"
FIRMWARE="$CACHE/blaster_6810.hex"

say() { printf '%s\n' "$*"; }
die() { printf 'de10: %s\n' "$*" >&2; exit 1; }

vm_running() { limactl list --format '{{.Status}}' "$VM" 2>/dev/null | grep -q Running; }

# run a command in the vm as the quartus user
in_vm() {
    vm_running || die "vm '$VM' is not running, run: $0 vm start"
    limactl shell "$VM" -- sudo -u user -H bash -lc "$1"
}

has_cyclonev() { in_vm "test -d $QROOT/quartus/common/devinfo/cyclonev" 2>/dev/null; }

board_on_usb() { ioreg -p IOUSB -l -w 0 | grep -q '"idVendor" = 2299'; }   # 0x09fb altera

cmd_doctor() {
    local ok=1
    check() { if eval "$2" >/dev/null 2>&1; then say "ok    $1"; else say "MISS  $1  -> $3"; ok=0; fi; }
    check "apple silicon mac" '[ "$(uname -m)" = arm64 ]' "these scripts assume an m-series mac"
    check "limactl" 'command -v limactl' "brew install lima"
    check "vm '$VM' exists" 'limactl list -q | grep -qx "$VM"' "run ~/dev/QuartusForMacOS/macos_helper_quartus.sh vz"
    check "vm '$VM' running" 'vm_running' "$0 vm start"
    if vm_running; then
        check "quartus in the vm" 'in_vm "test -x $QBIN/quartus_sh"' "reinstall quartus in the vm"
        check "cyclone v device support" 'has_cyclonev' "download the .qdz in a browser, then $0 install-cyclonev"
    fi
    check "openocd" 'command -v openocd' "brew install openocd"
    check "usb-blaster ii firmware" '[ -f "$FIRMWARE" ]' "$0 firmware"
    check "board on usb (usb-blaster ii)" 'board_on_usb' "plug the mini-usb cable into the port labelled USB Blaster (J13)"
    say ""
    say "power: the de10-nano is powered by its 5v dc barrel jack, not by usb."
    say "       the blue power led next to the jack must be on before detect/flash."
    [ $ok = 1 ] && say "all good" || say "fix the MISS lines above, top to bottom"
}

cmd_vm() {
    case "${1:-}" in
        start) vm_running && say "vm already running" || limactl start "$VM" ;;
        stop) limactl stop "$VM" ;;
        *) die "usage: $0 vm start|stop" ;;
    esac
}

cmd_install_cyclonev() {
    local qdz="${1:-$SHARED_MAC/cyclonev-20.1.1.720.qdz}"
    [ -f "$qdz" ] || die "no $qdz. download cyclonev-20.1.1.720.qdz (quartus lite 20.1.1, individual files) from the intel fpga download center in your browser, into $SHARED_MAC"
    case "$qdz" in "$SHARED_MAC"/*) ;; *) die "put the .qdz inside $SHARED_MAC so the vm can see it" ;; esac
    has_cyclonev && { say "cyclone v support already installed"; return; }
    local vmpath="$SHARED_VM/${qdz#"$SHARED_MAC"/}"
    # a .qdz is a zip of paths under the install root (quartus/common/devinfo/...).
    # check that before unpacking anything into the install.
    in_vm "unzip -Z1 '$vmpath' | head -n 3" | grep -q '^quartus/' \
        || die "$qdz doesn't look like a quartus device archive (expected paths under quartus/)"
    say "unpacking into $QROOT (a few minutes)..."
    in_vm "unzip -q -o '$vmpath' -d $QROOT"
    has_cyclonev && say "cyclone v support installed" || die "unpacked, but devinfo/cyclonev is still missing"
}

cmd_firmware() {
    mkdir -p "$CACHE"
    in_vm "cat $QROOT/quartus/linux64/blaster_6810.hex" > "$FIRMWARE"
    [ -s "$FIRMWARE" ] || die "could not copy blaster_6810.hex out of the vm"
    say "firmware at $FIRMWARE"
}

need_openocd() {
    command -v openocd >/dev/null || die "openocd missing: brew install openocd"
    [ -f "$FIRMWARE" ] || die "firmware missing: $0 firmware"
    board_on_usb || die "no usb-blaster on usb. is the board powered (5v jack) and the mini-usb in J13?"
}

cmd_detect() {
    need_openocd
    # no taps declared: openocd autoprobes and prints every idcode it finds
    DE10_FIRMWARE="$FIRMWARE" openocd -f "$HERE/blaster2.cfg" -c "init; scan_chain; shutdown"
}

# quartus deadlocks under the vm's x86 emulation when it runs threaded
single_thread() {
    grep -q NUM_PARALLEL_PROCESSORS "$1" && return
    printf '\nset_global_assignment -name NUM_PARALLEL_PROCESSORS 1\n' >> "$1"
}

# analysis + synthesis of the whole soc: proves the verilog synthesizes
# and reports its size. the vm sees the repo read-only at the same path.
cmd_synth() {
    local family="${1:-Cyclone IV E}" device="${2:-EP4CE115F29C7}"
    local repo; repo="$(cd "$HERE/../.." && pwd)"
    local dst="$SHARED_MAC/de10/soc_synth" f
    rm -rf "$dst" && mkdir -p "$dst"
    printf 'QUARTUS_VERSION = "20.1"\nPROJECT_REVISION = "soc_synth"\n' > "$dst/soc_synth.qpf"
    {
        printf 'set_global_assignment -name FAMILY "%s"\n' "$family"
        printf 'set_global_assignment -name DEVICE %s\n' "$device"
        printf 'set_global_assignment -name TOP_LEVEL_ENTITY soc\n'
        for f in section3/soc.v section3/arm7_cpu.v section5/mmu.v section5/timer.v section2/uart.v section4/ethernet.v; do
            printf 'set_global_assignment -name VERILOG_FILE %s/%s\n' "$repo" "$f"
        done
        printf 'set_global_assignment -name PROJECT_OUTPUT_DIRECTORY output_files\n'
    } > "$dst/soc_synth.qsf"
    single_thread "$dst/soc_synth.qsf"
    say "synthesizing the soc for $device in the vm..."
    in_vm "cd $SHARED_VM/de10/soc_synth && timeout 1800 $QBIN/quartus_map soc_synth" \
        | grep -E '^(Error|Critical Warning)|was successful|unsuccessful' || true
    say ""
    cat "$dst/output_files/soc_synth.map.summary" 2>/dev/null || die "no summary, see $dst/output_files/soc_synth.map.rpt"
}

cmd_compile() {
    local src="${1:-}"
    [ -d "$src" ] || die "usage: $0 compile <projdir>  (dir holding <name>.qpf)"
    src="$(cd "$src" && pwd)"
    local name; name="$(basename "$src")"
    [ -f "$src/$name.qpf" ] || die "$src/$name.qpf not found (the dir name must match the project name)"
    has_cyclonev || die "no cyclone v support in quartus yet: $0 install-cyclonev"
    local dst="$SHARED_MAC/de10/$name"
    mkdir -p "$dst"
    rsync -a --exclude db --exclude incremental_db --exclude output_files "$src/" "$dst/"
    single_thread "$dst/$name.qsf"
    say "compiling $name in the vm (slow: quartus runs under x86 emulation)..."
    in_vm "cd $SHARED_VM/de10/$name && timeout 1800 $QBIN/quartus_sh --flow compile $name"
    # svf for openocd: 12 mhz tck, 3.3 v, program
    in_vm "cd $SHARED_VM/de10/$name/output_files && $QBIN/quartus_cpf -c -q 12.0MHz -g 3.3 -n p $name.sof $name.svf"
    say "built $dst/output_files/$name.svf"
}

cmd_flash() {
    local svf="${1:-}"
    [ -f "$svf" ] || die "usage: $0 flash <file.svf>"
    need_openocd
    DE10_FIRMWARE="$FIRMWARE" openocd -f "$HERE/blaster2.cfg" -f "$HERE/de10_chain.cfg" \
        -c "init; svf -tap de10.fpga.tap {$svf} progress; shutdown"
}

cmd_blink() {
    cmd_compile "$HERE/blink"
    cmd_flash "$SHARED_MAC/de10/blink/output_files/blink.svf"
}

case "${1:-}" in
    doctor) cmd_doctor ;;
    vm) cmd_vm "${2:-}" ;;
    install-cyclonev) cmd_install_cyclonev "${2:-}" ;;
    firmware) cmd_firmware ;;
    detect) cmd_detect ;;
    compile) cmd_compile "${2:-}" ;;
    synth) cmd_synth "${2:-}" "${3:-}" ;;
    flash) cmd_flash "${2:-}" ;;
    blink) cmd_blink ;;
    *) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac
