// I was reading through the std library for random silly things and I found this , https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161
// this essentially provides a more rigorous foundation to my SWAR technique.

// code taken from https://github.com/gituser12981u2/memchr_stuff/blob/main/src/memchr_new.rs (my own work with a friend)
/*

READ

I was basically using this as a learning project, to do cool things, then I found an optimisation for memrchr that was nice

this code is a bit janky, not really important.

memrchr is significantly changed from stdlib implementation to use a more efficient swar method.

*/

use core::num::NonZeroUsize;
const USIZE_BYTES: usize = size_of::<usize>();
const USIZE_MINUS_1: usize = USIZE_BYTES - 1;
#[inline]
const fn repeat_u8(x: u8) -> usize {
    usize::from_ne_bytes([x; USIZE_BYTES])
}

const LO_USIZE: usize = repeat_u8(0x01);
const HI_USIZE: usize = repeat_u8(0x80);

// I am simply too lazy to comment all of these, it turns out a nice optimisation existed for memrchr
// I have done so, it seems the same optimisation is available for memchr but I need to work on the details
// Once done, I'll add it to the stdlib as a PR potentially
// https://github.com/gituser12981u2/memchr_stuff/blob/big_endian_fix/src/memchr_new.rs

// simplifying functioon
#[inline]
const fn find_last_nul(num: NonZeroUsize) -> usize {
    #[cfg(target_endian = "big")]
    {
        USIZE_MINUS_1 - ((num.trailing_zeros()) >> 3) as usize
    }

    #[cfg(target_endian = "little")]
    {
        USIZE_MINUS_1 - ((num.leading_zeros()) >> 3) as usize
    }
}

#[inline]
#[cfg(target_endian = "little")]
#[must_use]
const fn contains_zero_byte_borrow_fix(input: usize) -> Option<NonZeroUsize> {
    /*
    Hybrid approach:
    1) Use the classic SWAR test as a cheap early-out for the common case
       where there are no zero bytes.
    2) If the classic test indicates a possible match, compute a borrow/carry-
       safe mask that cannot produce cross-byte false positives. This matters
       for reverse search where we pick the *last* match.

    Classic SWAR: may contain false positives due to cross-byte borrow.
    However considering that we want to check *as quickly* as possible, this is ideal.
    */
    let mut classic = input.wrapping_sub(LO_USIZE) & !input & HI_USIZE;
    // We don't use the big endian formula because we need `!input` in scope
    // which allows for CSE in this expression
    if classic == 0 {
        return None;
    }
    /*
    This function occurs a branch here contains zero byte doesn't, it delegates the branch
    to the memchr(on LE) (or opposite on BE) function, this is okay because a *branch still occurs*

    Borrow-safe (carry-safe) SWAR:

    The classic HASZERO mask is perfect for a boolean “any zero byte?” check, but the *per-byte* mask
    can contain extra 0x80 bits when the subtraction `input - 0x01..` borrows across byte lanes.
    That’s a problem here because we don’t just test “non-zero?” — we feed the mask into
    `leading_zeros`/`trailing_zeros` to pick an actual byte index.

    Example (two adjacent bytes, lowest first):
    - `input = [0x00, 0x01]`
    - subtracting `0x01..` borrows from the `0x00` byte into the next byte, so the classic mask may
      report both bytes as candidates even though only the first byte is truly zero.

    `!input << 7` moves each byte’s low bit into that byte’s 0x80 position; bytes with LSB=1 (notably
    0x01, which is the common “borrow false-positive” case) get their candidate bit cleared.
    Due to CSE, `!input` is reused (EG on X86_64, register RDI is reused)
    Explanation further on https://github.com/gituser12981u2/memchr_stuff/blob/main/src/memchr_new.rs
    */
    classic &= !input << 7;
    /*
    SAFETY: `classic != 0` implies there is at least one real zero byte
    somewhere in the word (false positives only occur alongside a real zero
    due to borrow propagation), so `zero_mask` must be non-zero.
    Use this to get smarter intrinsic (aka ctlz/cttz non_zero)
    Note: Debug assertions check zero_mask!=0 so check tests for comprehensive validation
    */
    Some(unsafe { NonZeroUsize::new_unchecked(classic) })
}

#[inline]
#[cfg(target_endian = "big")]
const fn contains_zero_byte(input: usize) -> Option<NonZeroUsize> {
    // Classic HASZERO trick. (Mycroft)
    NonZeroUsize::new(input.wrapping_sub(LO_USIZE) & HI_USIZE & !input)
}

// This is an optimised version of memrchr

/// Returns the last index matching the byte `x` in `text`.
///
///# References
///- [Stanford Bit Twiddling Hacks find 0 byte ](http://www.icodeguru.com/Embedded/Hacker%27s-Delight/043.htm)
///- [Original memrchr implementation ](https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161)
#[must_use]
#[inline]
pub fn memrchr(x: u8, text: &[u8]) -> Option<usize> {
    // Scan for a single byte value by reading two `usize` words at a time.
    // Split `text` in three parts:
    // - unaligned tail, after the last word aligned address in text,
    // - body, scanned by 2 words at a time,
    // - the first remaining bytes, < 2 word size.

    const BLOCK_MASK: usize = 2 * USIZE_BYTES - 1;
    let slen = text.len();
    let start = text.as_ptr();
    // Number of bytes to reach the next `2*usize` boundary.
    let prefix = start.align_offset(const { 2 * USIZE_BYTES });

    // First 2-register-aligned offset, clamped for short inputs.
    let aligned_start = prefix.min(slen);

    // End of the largest 2-register-aligned region,slen>=aligned_start, never wraps
    let aligned_end = aligned_start + ((slen - aligned_start) & !BLOCK_MASK);
    // Simple scalar byte scan lambda, avoid iterator overhead.(Reverse iterators optimise quite badly plus we're specialising this by
    //specialising this, because we use the base_ptr and do basic pointer arithmetic, we can just already use existing registers(holding the base_ptr)
    let rposition_byte = |base: *const u8, len: usize| -> Option<usize> {
        // SAFETY: inbounds.
        let mut s = unsafe { base.add(len) };
        while s != base {
            // don't use >/>=, more assembly
            // SAFETY: s remains in [base, base + len).; Always s>base, never null,
            s = unsafe { s.sub(1) };
            // SAFETY: inbounds always
            if unsafe { s.read() } == x {
                //SAFETY:s >= start_ptr ; use direct pointer arithmetic instead of instantiating a loop counter
                return Some(unsafe { s.offset_from_unsigned(start) });
            }
        }

        None
        // equivalent expression, creates too  unoptimal asm.
        // unsafe {
        //     core::slice::from_raw_parts(base, len)
        //         .iter()
        //         .rfind(|y| **y == x) // fun pointer stuff lol.
        //         .map(|x| core::ptr::from_ref(x).byte_offset_from_unsigned(start))
        // }
    };

    // SAFETY: trivially within bounds
    // tail is [offset, slen)
    if let Some(i) = rposition_byte(unsafe { start.add(aligned_end) }, slen - aligned_end) {
        return Some(i);
    }
    let mut offset = aligned_end;

    let repeated_x = repeat_u8(x);

    // define another simple lambda  to avoid repetitive code.
    let check_usize = |strptr: *const usize| {
        // cast the upper/lower to a usize, we did all the math to make sure we're only reading aligned chunks,
        // we could simplify this with a deref but I want the code to be specifically indicating aigned reads (pedantic aside, just taste)
        // SAFETY: aligned+inbounds; read the pointer and XOR mask to set matching bits to 0 (and 0's to x)
        let upper_or_lower = unsafe { strptr.read() } ^ repeated_x;

        #[cfg(target_endian = "big")]
        let maybe_match = contains_zero_byte(upper_or_lower);
        #[cfg(target_endian = "little")]
        // because of borrow issues propagating to LSB we need to do a fix for LE, not for BE though, slight win?!
        let maybe_match = contains_zero_byte_borrow_fix(upper_or_lower);
        if let Some(num) = maybe_match {
            let zero_byte_pos = find_last_nul(num);
            //specifically use pointer arithmetic directly, it optimises nicer quite often, a lot less data depencies!
            // SAFETY: strptr>=start_ptr by definition; calculate distance from current pointer to base ptr, then add the zero_byte_pos and boom.
            return Some(unsafe { strptr.byte_offset_from_unsigned(start) } + zero_byte_pos);
        }
        None
    };
    /*
    Search the body of the text, make sure we don't cross aligned_start.
    offset is always aligned, so just testing `>` is sufficient and avoids possible overflow.
    NBD: using  = optimises worse but is equivalent.
    */

    while offset > aligned_start {
        offset -= USIZE_BYTES;
        // SAFETY: always in bounds, checking invariants.
        unsafe { debug_assert!(start.add(offset).cast::<usize>().is_aligned(), "check") };

        // SAFETY: Trivially inbounds+aligned reading upper word avoid the cost of unaligned reads(same as memchr/memrchr in STD)
        if let Some(valid) = unsafe { check_usize(start.byte_add(offset).cast::<usize>()) } {
            return Some(valid);
        }

        offset -= USIZE_BYTES;
        // SAFETY: as above
        if let Some(valid) = unsafe { check_usize(start.byte_add(offset).cast::<usize>()) } {
            return Some(valid);
        }
    }
    debug_assert!(offset == aligned_start, "test"); // they're always equal, but llvm optimisesr better using offset instead of aligned start because it repurposes a register
    // The character we were looking for didn't appear in the aligned body, do a simple loop to check the head segment.
    rposition_byte(start, offset)
}

// a sketch i liked
// #[must_use]
// #[inline(never)]
// pub fn memchr(x: u8, text: &[u8]) -> Option<usize> {
//     // Scan for a single byte value by reading two `usize` words at a time.
//     //
//     // Split `text` into three parts:
//     // - prefix: bytes before the first `usize`-aligned address,
//     // - body: aligned `usize` words, scanned two words at a time,
//     // - suffix: the remaining bytes, fewer than two `usize` words.

//     const BLOCK_MASK: usize = 2 * USIZE_BYTES - 1;

//     let slen = text.len();
//     let start = text.as_ptr();

//     // Number of bytes to reach the next `usize` boundary.
//     let prefix = start.align_offset(2 * USIZE_BYTES);

//     // First word-aligned offset, clamped for short inputs.
//     let aligned_start = prefix.min(slen);

//     // End of the largest region containing a whole number of two-word blocks.
//     //
//     // `slen >= aligned_start`, so the subtraction cannot underflow.
//     // The addition cannot wrap because the result is <= `slen`.
//     let aligned_end = aligned_start + ((slen - aligned_start) & !BLOCK_MASK);

//     let position_byte = |base: *const u8, len: usize| -> Option<usize> {
//         let end = unsafe { base.add(len) };
//         let mut s = base;

//         while s != end {
//             // SAFETY: `s` is always in `[base, end)`, so this read is in bounds.
//             if unsafe { s.read() } == x {
//                 // SAFETY: `s` is within the original `text`, so the distance
//                 // from `start` is in bounds and represents an index into `text`.
//                 return Some(unsafe { s.byte_offset_from_unsigned(start) });
//             }

//             // SAFETY: `s != end`, so advancing by one remains within the
//             // allocation represented by `text`.
//             s = unsafe { s.add(1) };
//         }

//         None
//     };

//     // Scan the unaligned prefix.
//     if let Some(i) = position_byte(start, aligned_start) {
//         return Some(i);
//     }

//     let repeated_x = repeat_u8(x);

//     let check_usize = |strptr: *const usize| -> Option<usize> {
//         // SAFETY: callers only pass a `usize`-aligned pointer into the
//         // fully-aligned body, with a complete `usize` word remaining.
//         let upper_or_lower = unsafe { strptr.read() } ^ repeated_x;

//         #[cfg(target_endian = "big")]
//         let maybe_match = contains_zero_byte_borrow_fix(upper_or_lower);

//         #[cfg(target_endian = "little")]
//         let maybe_match = contains_zero_byte(upper_or_lower);

//         if let Some(num) = maybe_match {
//             let zero_byte_pos = find_first_nul(num);

//             // SAFETY: `strptr` points into `text` at or after `start`.
//             // `zero_byte_pos` is an offset within the word just read.
//             return Some(unsafe { strptr.byte_offset_from_unsigned(start) } + zero_byte_pos);
//         }

//         None
//     };

//     let mut offset = aligned_start;

//     // Scan the aligned body two words at a time.
//     while offset < aligned_end {
//         // First word.
//         if let Some(valid) = unsafe { check_usize(start.byte_add(offset).cast::<usize>()) } {
//             return Some(valid);
//         }

//         offset += USIZE_BYTES;

//         // Second word.
//         if let Some(valid) = unsafe { check_usize(start.byte_add(offset).cast::<usize>()) } {
//             return Some(valid);
//         }

//         offset += USIZE_BYTES;
//     }

//     // Scan the remaining < 2 * USIZE_BYTES bytes.
//     position_byte(unsafe { start.byte_add(offset) }, slen - offset)
// }
