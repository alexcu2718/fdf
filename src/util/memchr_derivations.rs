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
        USIZE_BYTES - 1 - ((num.trailing_zeros()) >> 3) as usize
    }

    #[cfg(target_endian = "little")]
    {
        USIZE_BYTES - 1 - ((num.leading_zeros()) >> 3) as usize
    }
}

/// Returns the last index matching the byte `x` in `text`.
#[inline]
// Check assembly to see if we need this Adrian, you did it lol.
// 1 fewer instruction using this, need to look at more.
const unsafe fn rposition_byte_len(base: *const u8, len: usize, needle: u8) -> Option<usize> {
    let mut i = len;
    while i != 0 {
        i -= 1;
        // SAFETY: trivially within bounds
        if unsafe { base.add(i).read() } == needle {
            return Some(i);
        }
    }
    None
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
// Only for BE
const fn contains_zero_byte(input: usize) -> Option<NonZeroUsize> {
    // Classic HASZERO trick. (Mycroft)
    NonZeroUsize::new(input.wrapping_sub(LO_USIZE) & HI_USIZE & !input)
}

// This is an optimised version of memrchr. As part of a *potential* commit towards stdlib.

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

    let len = text.len();

    let start_ptr = text.as_ptr();

    const ALIGN_MASK: usize = USIZE_BYTES - 1;
    const BLOCK_MASK: usize = 2 * USIZE_BYTES - 1;

    let prefix = start_ptr.addr().wrapping_neg() & ALIGN_MASK;
    let min_aligned_offset = prefix.min(len);
    let max_aligned_offset = min_aligned_offset + (len.saturating_sub(prefix) & !BLOCK_MASK);
    // classic align in C AKA
    // #define  align_up(x) (x & ~(sizeof(size_t)-1)) //(MUST be power of 2 )

    let mut offset = max_aligned_offset;

    let tail_len = len - offset; // tail is [offset, len)
    // SAFETY: trivially within bounds
    if let Some(i) = unsafe { rposition_byte_len(start_ptr.add(offset), tail_len, x) } {
        return Some(offset + i);
    }

    let repeated_x = repeat_u8(x);

    // define a simple function to avoid repetitive code.
    #[inline]
    const unsafe fn check_usize(strptr: *const u8, offset: usize, mask: usize) -> Option<usize> {
        // SAFETY: Always in bounds+aligned so read is valid.
        let upper_or_lower = unsafe { strptr.add(offset).cast::<usize>().read() ^ mask };
        // mask to turn all matching bytes to 0 and 0's to `x`

        #[cfg(target_endian = "big")]
        let maybe_match = contains_zero_byte(upper_or_lower);
        #[cfg(target_endian = "little")]
        // because of borrow issues propagating to LSB we need to do a fix for LE, not for BE though, slight win?!
        let maybe_match = contains_zero_byte_borrow_fix(upper_or_lower);

        if let Some(num) = maybe_match {
            let zero_byte_pos = find_last_nul(num);

            return Some(offset + zero_byte_pos);
        }
        None
    }

    /*
    Search the body of the text, make sure we don't cross min_aligned_offset.
    offset is always aligned, so just testing `>` is sufficient and avoids possible overflow.
    */

    while offset > min_aligned_offset {
        offset -= USIZE_BYTES;

        // SAFETY: the body is trivially aligned due to align_to, avoid the cost of unaligned reads(same as memchr/memrchr in STD)
        if let Some(valid) = unsafe { check_usize(start_ptr, offset, repeated_x) } {
            return Some(valid);
        }

        offset -= USIZE_BYTES;
        // SAFETY: as above
        if let Some(valid) = unsafe { check_usize(start_ptr, offset, repeated_x) } {
            return Some(valid);
        }
    }
    // The character we were looking for didn't appear in the aligned body, do a simple loop to check the head segment.
    // SAFETY: trivially within bounds
    unsafe { rposition_byte_len(start_ptr, offset, x) }
}
