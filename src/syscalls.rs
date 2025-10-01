// honestly i should probably delete this and rely on just libc but I ideally would like to not dynamically link glibc in future on  Linux
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
#![allow(clippy::undocumented_unsafe_blocks)] // too lazy to comment it all
use core::ffi::CStr;

//can comment all of this later
use crate::ValueType;
#[inline]
#[allow(clippy::multiple_unsafe_ops_per_block)]
#[must_use]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
// x86_64-specific implementation
/// Opens a directory using direct syscall via assembly
///
///
/// # Safety
/// - Requires cstr to be a valid directory
///
/// # Returns
/// - File descriptor (positive integer) on success
/// - -1 on error (check errno for details)
<<<<<<< Updated upstream
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    use std::arch::asm;
    // Create null-terminated C string from byte slice
    let filename: *const u8 = unsafe { cstr!(bytepath) };
    const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
=======
pub unsafe fn open_asm(cstr: &CStr, flags: i32) -> i32 {
    use core::arch::asm;
>>>>>>> Stashed changes
    const SYSCALL_NUM: i32 = libc::SYS_open as _;

    let fd: i32;
    unsafe {
        asm!(
            "syscall",
            inout("rax") SYSCALL_NUM => fd,
            in("rdi") cstr.as_ptr()  ,
            in("rsi") flags,
            in("rdx") libc::O_RDONLY,
            out("rcx") _, out("r11") _,
             // Clobbered registers (resetting)
            options(nostack, preserves_flags)
        )
    };
    fd
}

#[inline]
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
/// Opens a directory using `openat` syscall via assembly
///
/// ARM64 uses `openat` instead of `open` for better path resolution.
/// Uses `AT_FDCWD` to indicate relative to current working directory.
///
/// # Safety
/// - Requires byte path to be a valid directory
///
/// # Returns
/// File descriptor on success, -1 on error
<<<<<<< Updated upstream
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    use std::arch::asm;
    let filename: *const u8 = unsafe { cstr!(bytepath) };
=======
pub unsafe fn open_asm(cstr: &CStr, flags: i32) -> i32 {
    use core::arch::asm;
>>>>>>> Stashed changes

    const MODE: i32 = libc::O_RDONLY; // Required even if unused in directory open
    const SYSCALL_OPENAT: i32 = libc::SYS_openat as i32;

    let fd: i32;
    unsafe {
        asm!(
            "svc 0",
            in("x0") libc::AT_FDCWD,          // dirfd = AT_FDCWD
            in("x1") cstr.as_ptr(),
            in("x2") flags,
            in("x3") MODE,
            in("x8") SYSCALL_OPENAT,
            lateout("x0") fd,           // return value
            options(nostack)
        )
    };
    fd
}

#[inline]
#[cfg(all(
    target_os = "linux",
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
/// Opens a directory using `libc::open`
///
/// # Safety
/// - Requires byte path to be a valid directory
///
/// # Returns
/// File descriptor on success, -1 on error
pub unsafe fn open_asm(cstr: &CStr, flags: i32) -> i32 {
    unsafe { libc::open(cstr.as_ptr(), flags) }
}

#[inline]
#[cfg(all(
    target_os = "linux",
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
/// Closes file descriptor using direct syscall
///
/// # Safety
/// - Takes ownership of the file descriptor
/// - Invalidates fd after call
// (we can't check error without A LOT of unnecessarily work, so once we attempt a close, it's over, accept it( we don't want to overcomplicate something simple)
pub unsafe fn close_asm(fd: i32) {
    unsafe { libc::close(fd) }; //this is a procedure so there can't be a ret value
}

#[inline]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
/// Close directory using direct assembly instructions
/// # Safety
/// - Takes ownership of the file descriptor
/// - Invalidates fd after call (even on error)
pub unsafe fn close_asm(fd: i32) {
    use std::arch::asm;
    let _: isize;
    unsafe {
        asm!(
            "syscall",
            inout("rax") libc::SYS_close => _,
            in("rdi") fd,
            out("rcx") _, out("r11") _,//ignore return
            options(nostack, preserves_flags, nomem)
            //no stack operations,no memory access
        )
    };
}

#[inline]
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
/// Closes file descriptor using direct syscall
///
/// Follows ARM64 syscall conventions:
/// - Syscall number in x8
/// - First argument in x0
///
/// # Safety
/// - Takes ownership of the file descriptor
/// - Invalidates fd after call (even on error)
/// - No error checking - intentional for performance
pub unsafe fn close_asm(fd: i32) {
    use std::arch::asm;
    const SYSCALL_CLOSE: i32 = libc::SYS_close as _;

    unsafe {
        asm!(
            "svc 0",
            in("x0") fd,              // File descriptor (first argument)
            in("x8") SYSCALL_CLOSE,   // Syscall number in x8
            lateout("x0") _,           // Ignore return value (we've used this register so we need to explicitly invalidate it)
            options(nostack, nomem)    // No stack operations or memory access
        );
    }
}

#[inline]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
/// Reads directory entries using `getdents64` syscall (no libc) for `x86_64/aarm64` (failing that, libc)
///
/// # Arguments
/// - `fd`: Open directory file descriptor
/// - `buffer_ptr`: Raw pointer to output buffer
/// - `buffer_size`: Size of output buffer in bytes
///
/// # Safety
/// - Requires valid open directory descriptor
/// - Buffer must be valid for writes of `buffer_size` bytes
/// - No type checking on generic pointer
///
/// # Returns
/// - Positive: Number of bytes read
/// - 0: End of directory
/// - Negative: Error code (check errno)
pub unsafe fn getdents_asm<T>(fd: i32, buffer_ptr: *mut T, buffer_size: usize) -> i64
where
    T: ValueType, //i8/u8
{
    use std::arch::asm;
    let output;
    unsafe {
        asm!(
            "syscall",
            inout("rax") libc::SYS_getdents64  => output,
            in("rdi") fd, //i'd put comments but these are pretty trivial
            in("rsi") buffer_ptr,
            in("rdx") buffer_size,
            out("rcx") _,  // syscall clobbers rcx
            out("r11") _,  // syscall clobbers r11
            options(nostack, preserves_flags)
        )
    };

    output
}

#[inline]
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
/// Reads directory entries using `getdents64` syscall (no libc) for aarch64
///
/// # Arguments
/// - `fd`: Open directory file descriptor
/// - `buffer_ptr`: Raw pointer to output buffer
/// - `buffer_size`: Size of output buffer in bytes
///
/// # Safety
/// - Requires valid open directory descriptor
/// - Buffer must be valid for writes of `buffer_size` bytes
/// - No type checking on generic pointer(must be i8/u8)
///
/// # Returns
/// - Positive: Number of bytes read
/// - 0: End of directory
/// - Negative: Error code (check errno)
pub unsafe fn getdents_asm<T>(fd: i32, buffer_ptr: *mut T, buffer_size: usize) -> i64
where
    T: ValueType, //i8/u8
{
    use std::arch::asm;
    let ret: i64;
    unsafe {
        asm!(
            "svc 0", // Supervisor call
            inout("x0") fd as i64 => ret,  // fd argument and return value
            in("x1") buffer_ptr , //self explanatory naming wins!
            in("x2") buffer_size,//^
            in("x8") libc::SYS_getdents64 as i64,//casting an appropriate
            out("x16") _,
            out("x17") _,
            options(nostack, nomem)
        );
    }
    ret
}

#[inline]
#[cfg(all(
    target_os = "linux",
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
/// Libc-based fallback for reading directory entries
/// # Arguments
/// - `fd`: Open directory file descriptor
/// - `buffer_ptr`: Raw pointer to output buffer
/// - `buffer_size`: Size of output buffer in bytes
///
/// # Safety
/// - Requires valid open directory descriptor
/// - Buffer must be valid for writes of `buffer_size` bytes
/// - No type checking on generic pointer(T  must be i8/u8)
///
/// # Returns
/// - Positive: Number of bytes read
/// - 0: End of directory
/// - Negative: Error code (check errno)
pub unsafe fn getdents_asm<T>(fd: i32, buffer_ptr: *mut T, buffer_size: usize) -> i64
where
    T: ValueType, //i8/u8
{
    unsafe { libc::syscall(libc::SYS_getdents64, fd, buffer_ptr, buffer_size) }
}

/*

reference for when I feel like writing RISC https://man7.org/linux/man-pages/man2/syscall.2.html
   return the system call result, and the register used to signal an
       error.
       Arch/ABI    Instruction           System  Ret  Ret  Error    Notes
                                         call #  val  val2
       ───────────────────────────────────────────────────────────────────
       alpha       callsys               v0      v0   a4   a3       1, 6
       arc         trap0                 r8      r0   -    -
       arm/OABI    swi NR                -       r0   -    -        2
       arm/EABI    swi 0x0               r7      r0   r1   -
       arm64       svc #0                w8      x0   x1   -
       blackfin    excpt 0x0             P0      R0   -    -
       i386        int $0x80             eax     eax  edx  -
       ia64        break 0x100000        r15     r8   r9   r10      1, 6
       loongarch   syscall 0             a7      a0   -    -
       m68k        trap #0               d0      d0   -    -
       microblaze  brki r14,8            r12     r3   -    -
       mips        syscall               v0      v0   v1   a3       1, 6
       nios2       trap                  r2      r2   -    r7
       parisc      ble 0x100(%sr2, %r0)  r20     r28  -    -
       powerpc     sc                    r0      r3   -    r0       1
       powerpc64   sc                    r0      r3   -    cr0.SO   1
       riscv       ecall                 a7      a0   a1   -
       s390        svc 0                 r1      r2   r3   -        3
       s390x       svc 0                 r1      r2   r3   -        3
       superh      trapa #31             r3      r0   r1   -        4, 6
       sparc/32    t 0x10                g1      o0   o1   psr/csr  1, 6
       sparc/64    t 0x6d                g1      o0   o1   psr/csr  1, 6
       tile        swint1                R10     R00  -    R01      1
       x86-64      syscall               rax     rax  rdx  -        5
       x32         syscall               rax     rax  rdx  -        5
       xtensa      syscall               a2      a2   -    -

       Notes:




https://doc.rust-lang.org/reference/inline-assembly.html (for my own reference)

[asm.register-operands.supported-register-classes]

Here is the list of currently supported register classes:
Architecture	Register class	Registers	LLVM constraint code
x86	reg	ax, bx, cx, dx, si, di, bp, r[8-15] (x86-64 only)	r
x86	reg_abcd	ax, bx, cx, dx	Q
x86-32	reg_byte	al, bl, cl, dl, ah, bh, ch, dh	q
x86-64	reg_byte*	al, bl, cl, dl, sil, dil, bpl, r[8-15]b	q
x86	xmm_reg	xmm[0-7] (x86) xmm[0-15] (x86-64)	x
x86	ymm_reg	ymm[0-7] (x86) ymm[0-15] (x86-64)	x
x86	zmm_reg	zmm[0-7] (x86) zmm[0-31] (x86-64)	v
x86	kreg	k[1-7]	Yk
x86	kreg0	k0	Only clobbers
x86	x87_reg	st([0-7])	Only clobbers
x86	mmx_reg	mm[0-7]	Only clobbers
x86-64	tmm_reg	tmm[0-7]	Only clobbers
AArch64	reg	x[0-30]	r
AArch64	vreg	v[0-31]	w
AArch64	vreg_low16	v[0-15]	x
AArch64	preg	p[0-15], ffr	Only clobbers
Arm64EC	reg	x[0-12], x[15-22], x[25-27], x30	r
Arm64EC	vreg	v[0-15]	w
Arm64EC	vreg_low16	v[0-15]	x
ARM (ARM/Thumb2)	reg	r[0-12], r14	r
ARM (Thumb1)	reg	r[0-7]	r
ARM	sreg	s[0-31]	t
ARM	sreg_low16	s[0-15]	x
ARM	dreg	d[0-31]	w
ARM	dreg_low16	d[0-15]	t
ARM	dreg_low8	d[0-8]	x
ARM	qreg	q[0-15]	w
ARM	qreg_low8	q[0-7]	t
ARM	qreg_low4	q[0-3]	x
RISC-V	reg	x1, x[5-7], x[9-15], x[16-31] (non-RV32E)	r
RISC-V	freg	f[0-31]	f
RISC-V	vreg	v[0-31]	Only clobbers
LoongArch	reg	$r1, $r[4-20], $r[23,30]	r
LoongArch	freg	$f[0-31]	f
s390x	reg	r[0-10], r[12-14]	r
s390x	reg_addr	r[1-10], r[12-14]	a
s390x	freg	f[0-15]	f
s390x	vreg	v[0-31]	Only clobbers
s390x	areg	a[2-15]	Only clobbers
*/
