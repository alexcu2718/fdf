#![allow(clippy::host_endian_bytes)]
#![allow(clippy::multiple_unsafe_ops_per_block)]

// I was reading through the std library for random silly things and I found this , https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161
// this essentially provides a more rigorous foundation to my SWAR technique.

// code taken from https://github.com/gituser12981u2/memchr_stuff/blob/main/src/memchr_new.rs (my own work with a friend)
/*

READ

I was basically using this as a learning project, to do cool things, then I found an optimisation for memrchr that was nice

this code is extremely janky and REALLY not contiguous to this code base, but because i'm learning, it's  fun!

memrchr is significantly changed from stdlib implementation to use a more efficient swar method.

*/

use core::num::NonZeroU64;
use core::num::NonZeroUsize;
#[inline]
const fn repeat_u8(x: u8) -> usize {
    usize::from_ne_bytes([x; size_of::<usize>()])
}

const LO_USIZE: usize = repeat_u8(0x01);
const HI_USIZE: usize = repeat_u8(0x80);

const LO_U64: u64 = repeat_u64(0x01);

const HI_U64: u64 = repeat_u64(0x80);

const USIZE_BYTES: usize = size_of::<usize>();

// I am simply too lazy to comment all of these, it turns out a nice optimisation existed for memrchr
// I have done so, it seems the same optimisation is available for memchr but I need to work on the details
// Once done, I'll add it to the stdlib as a PR potentially
// https://github.com/gituser12981u2/memchr_stuff/blob/big_endian_fix/src/memchr_new.rs

// simplifying macro
macro_rules! find_last_NUL {
    // SWAR
    ($num:expr) => {{
        #[cfg(target_endian = "big")]
        {
            (USIZE_BYTES - 1 - (($num.trailing_zeros()) >> 3) as usize)
        }
        #[cfg(target_endian = "little")]
        {
            (USIZE_BYTES - 1 - (($num.leading_zeros()) >> 3) as usize)
        }
    }};
}

macro_rules! find_first_NUL {
    // SWAR
    ($num:expr) => {{
        #[cfg(target_endian = "big")]
        {
            ($num.leading_zeros() >> 3) as usize
        }
        #[cfg(target_endian = "little")]
        {
            ($num.trailing_zeros() >> 3) as usize
        }
    }};
}

#[inline]
const fn repeat_u64(byte: u8) -> u64 {
    u64::from_ne_bytes([byte; size_of::<u64>()])
}

/**
 Finds the first occurrence of a byte in a 64-bit word.

 This uses a bitwise technique to locate the first instance of
 the target byte `c` in the 64-bit value `str`. The operation works by:

 (use `unwrap_unchecked` for truly branchless if you know it contains the character you're after)

 # Examples
```
use fdf::util::find_char_in_word;

// Helper function to create byte arrays from strings
fn create_byte_array(s: &str) -> [u8; 8] {
let mut bytes = [0u8; 8];
let s_bytes = s.as_bytes();
let len = s_bytes.len().min(8);
bytes[..len].copy_from_slice(&s_bytes[..len]);
bytes
}

// Basic usage
 let bytes = create_byte_array("hello");
assert_eq!(find_char_in_word(b'h', bytes), Some(0),"hello is predicted wrong!");

// Edge cases
assert_eq!(find_char_in_word(b'A', create_byte_array("AAAAAAAA")), Some(0)); // first position
assert_eq!(find_char_in_word(b'A', create_byte_array("")), None); // not found
assert_eq!(find_char_in_word(0, create_byte_array("\x01\x02\x03\0\x05\x06\x07\x08")), Some(3)); // null byte

// Multiple occurrences (returns first)
let bytes = create_byte_array("hello");
assert_eq!(find_char_in_word(b'l', bytes), Some(2)); // first 'l'
```

# Parameters
- `c`: The byte to search for (0-255)
- `bytestr`: The word ( a `[u8; 8]` ) to search in (64 bit specific)

# Returns
- `Some(usize)`: Index (0-7) of the first occurrence
- `None`: If the byte is not found
*/
#[inline]
#[must_use]
pub const fn find_char_in_word(c: u8, bytestr: [u8; 8]) -> Option<usize> {
    let xor_result = u64::from_ne_bytes(bytestr) ^ repeat_u64(c);
    #[cfg(target_endian = "little")]
    let swarred = NonZeroU64::new(xor_result.wrapping_sub(LO_U64) & !xor_result & HI_U64);

    // Avoid borrow issues on BE
    #[cfg(target_endian = "big")]
    let swarred =
        NonZeroU64::new((!xor_result & !HI_U64).wrapping_add(LO_U64) & (!xor_result & HI_U64));
    /*
    If you're asking why `NonZeroU64`, check `dirent_const_time_strlen` for more info.
    https://doc.rust-lang.org/src/core/num/nonzero.rs.html#599
    https://doc.rust-lang.org/beta/std/intrinsics/fn.ctlz_nonzero.html
    https://doc.rust-lang.org/beta/std/intrinsics/fn.cttz_nonzero.html
    */

    match swarred {
        Some(valid) => Some(find_first_NUL!(valid)),
        None => None,
    }
}

/**
 Finds the last occurrence of a byte in a 64-bit word.

 This uses a bitwise technique to locate the last instance of
 the target byte `c` in the 64-bit array `str`

 (use `unwrap_unchecked` for truly branchless if you know it contains the character you're after)

```
use fdf::util::find_last_char_in_word;

// Helper function to create byte arrays from strings
fn create_byte_array(s: &str) -> [u8; 8] {
let mut bytes = [0u8; 8];
let s_bytes = s.as_bytes();
let len = s_bytes.len().min(8);
bytes[..len].copy_from_slice(&s_bytes[..len]);
bytes
}

// Basic usage
 let bytes = create_byte_array("hello");
assert_eq!(find_last_char_in_word(b'h', bytes), Some(0),"hello is predicted wrong!");

// Edge cases
assert_eq!(find_last_char_in_word(b'A', create_byte_array("AAAAAAAA")), Some(7)); // last position
assert_eq!(find_last_char_in_word(b'A', create_byte_array("")), None); // not found
assert_eq!(find_last_char_in_word(0, create_byte_array("\x01\x02\x03\0\x05\x06\x07\x08")), Some(3)); // null byte

// Multiple occurrences (returns last )
let bytes = create_byte_array("hello");
assert_eq!(find_last_char_in_word(b'l', bytes), Some(3)); // last 'l'

let new_bytes = create_byte_array("he..eop");
assert_eq!(find_last_char_in_word(b'e', new_bytes), Some(4)); // last 'e'
```

# Parameters
- `c`: The byte to search for (0-255)
- `bytestr`: The word ( a `[u8; 8]` ) to search in (64 bit specific)

# Returns
- `Some(usize)`: Index (0-7) of the last occurrence
- `None`: If the byte is not found
*/
#[inline]
#[must_use]
pub const fn find_last_char_in_word(c: u8, bytestr: [u8; 8]) -> Option<usize> {
    //http://www.icodeguru.com/Embedded/Hacker%27s-Delight/043.htm
    // https://github.com/gituser12981u2/memchr_stuff/blob/big_endian_fix/src/memchr_new.rs
    // I am too lazy to type this out again. Check the line containing `The position of the rightmost 0-byte is given by t`
    let x = u64::from_ne_bytes(bytestr) ^ repeat_u64(c);
    #[cfg(target_endian = "little")]
    let swarred = (!x & !HI_U64).wrapping_add(LO_U64) & (!x & HI_U64);
    #[cfg(target_endian = "big")]
    let swarred = x.wrapping_sub(LO_U64) & !x & HI_U64;

    match NonZeroU64::new(swarred) {
        Some(num) => Some(find_last_NUL!(num)),
        None => None,
    }
}

/// Returns the last index matching the byte `x` in `text`.
///
/// This is an optimised version of memrchr. As part of a *potential* commit towards stdlib.
//# References
// - [Stanford Bit Twiddling Hacks find 0 byte ](http://www.icodeguru.com/Embedded/Hacker%27s-Delight/043.htm)
// - [Original memrchr implementation ](https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161)
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
#[allow(unused)] // only needed for LE
#[must_use]
const fn contains_zero_byte_borrow_fix(input: usize) -> Option<NonZeroUsize> {
    // Hybrid approach:
    // 1) Use the classic SWAR test as a cheap early-out for the common case
    //    where there are no zero bytes.
    // 2) If the classic test indicates a possible match, compute a borrow/carry-
    //    safe mask that cannot produce cross-byte false positives. This matters
    //    for reverse search where we pick the *last* match.

    // Classic SWAR: may contain false positives due to cross-byte borrow.
    // However considering that we want to check *as quickly* as possible, this is ideal.

    let mut classic = input.wrapping_sub(LO_USIZE) & (!input) & HI_USIZE;
    if classic == 0 {
        return None;
    }
    // This function occurs a branch here contains zero byte doesn't, it delegates the branch
    // to the memchr(on LE) (or opposite on BE) function, this is okay because a *branch still occurs*

    // Borrow-safe (carry-safe) SWAR:
    //
    // The classic HASZERO mask is perfect for a boolean “any zero byte?” check, but the *per-byte* mask
    // can contain extra 0x80 bits when the subtraction `input - 0x01..` borrows across byte lanes.
    // That’s a problem here because we don’t just test “non-zero?” — we feed the mask into
    // `leading_zeros`/`trailing_zeros` to pick an actual byte index.
    //
    // Example (two adjacent bytes, lowest first):
    // - `input = [0x00, 0x01]`
    // - subtracting `0x01..` borrows from the `0x00` byte into the next byte, so the classic mask may
    //   report both bytes as candidates even though only the first byte is truly zero.
    //
    // `!input << 7` moves each byte’s low bit into that byte’s 0x80 position; bytes with LSB=1 (notably
    // 0x01, which is the common “borrow false-positive” case) get their candidate bit cleared.
    // Due to CSE, `!input` is reused (EG on X86_64, register RDI is reused)
    // Explanation further on https://github.com/gituser12981u2/memchr_stuff/blob/main/src/memchr_new.rs (my own work)
    classic &= !input << 7;

    // SAFETY: `classic != 0` implies there is at least one real zero byte
    // somewhere in the word (false positives only occur alongside a real zero
    // due to borrow propagation), so `zero_mask` must be non-zero.
    // Use this to get smarter intrinsic (aka ctlz/cttz non_zero)
    // Note: Debug assertions check zero_mask!=0 so check tests for comprehensive validation
    Some(unsafe { NonZeroUsize::new_unchecked(classic) })
}

#[inline]
#[cfg(target_endian = "big")]
// Only for BE
const fn contains_zero_byte(input: usize) -> Option<NonZeroUsize> {
    // Classic HASZERO trick. (Mycroft)
    NonZeroUsize::new(input.wrapping_sub(LO_USIZE) & (!input) & HI_USIZE)
}

/// Returns the last index matching the byte `x` in `text`.
#[must_use]
#[inline]
#[expect(clippy::cast_ptr_alignment, reason = "alignment guaranteed")]
pub fn memrchr(x: u8, text: &[u8]) -> Option<usize> {
    // Scan for a single byte value by reading two `usize` words at a time.

    //

    // Split `text` in three parts:

    // - unaligned tail, after the last word aligned address in text,

    // - body, scanned by 2 words at a time,

    // - the first remaining bytes, < 2 word size.

    let len = text.len();

    let ptr = text.as_ptr();

    let (min_aligned_offset, max_aligned_offset) = {
        // We call this just to obtain the length of the prefix and suffix.

        // In the middle we always process two chunks at once.

        // SAFETY: transmuting `[u8]` to `[usize]` is safe except for size differences

        // which are handled by `align_to`.

        let (prefix, _, suffix) = unsafe { text.align_to::<(usize, usize)>() };

        (prefix.len(), len - suffix.len())
    };

    let mut offset = max_aligned_offset;

    let start = text.as_ptr();
    let tail_len = len - offset; // tail is [offset, len)
    // SAFETY: trivially within bounds
    if let Some(i) = unsafe { rposition_byte_len(start.add(offset), tail_len, x) } {
        return Some(offset + i);
    }
    /*
    This adds an extra ~10 instructions!(on x86 v1) (from std.) definitely worthwhile to avoid!

     if let Some(index) = text[offset..].iter().rposition(|elt| *elt == x) {
        return Some(offset + index);
    }


     */

    // Search the body of the text, make sure we don't cross min_aligned_offset.

    // offset is always aligned, so just testing `>` is sufficient and avoids possible

    // overflow.

    let repeated_x = repeat_u8(x);

    while offset > min_aligned_offset {
        // SAFETY: offset starts at len - suffix.len(), as long as it is greater than
        // min_aligned_offset (prefix.len()) the remaining distance is at least 2 * chunk_bytes.
        // SAFETY: the body is trivially aligned due to align_to, avoid the cost of unaligned reads(same as memchr/memrchr in STD)
        let lower = unsafe { ptr.add(offset - 2 * USIZE_BYTES).cast::<usize>().read() };
        // SAFETY: as above
        let upper = unsafe { ptr.add(offset - USIZE_BYTES).cast::<usize>().read() };

        // Break if there is a matching byte.
        // **CHECK UPPER FIRST**
        //XOR to turn the matching bytes to NUL
        // This swar algorithm has the benefit of not propagating 0xFF rightwards/leftwards after a match is found

        #[cfg(target_endian = "big")]
        let maybe_match_upper = contains_zero_byte(upper ^ repeated_x);
        #[cfg(target_endian = "little")]
        // because of borrow issues propagating to LSB we need to do a fix for LE, not for BE though, slight win?!
        let maybe_match_upper = contains_zero_byte_borrow_fix(upper ^ repeated_x);

        if let Some(num) = maybe_match_upper {
            let zero_byte_pos = find_last_NUL!(num);

            return Some(offset - USIZE_BYTES + zero_byte_pos);
        }

        #[cfg(target_endian = "big")]
        let maybe_match_lower = contains_zero_byte(lower ^ repeated_x);
        #[cfg(target_endian = "little")]
        let maybe_match_lower = contains_zero_byte_borrow_fix(lower ^ repeated_x);

        if let Some(num) = maybe_match_lower {
            // replace this function
            let zero_byte_pos = find_last_NUL!(num);

            return Some(offset - 2 * USIZE_BYTES + zero_byte_pos);
        }

        offset -= 2 * USIZE_BYTES;
    }
    // SAFETY: trivially within bounds
    // Find the byte before the point the body loop stopped.
    unsafe { rposition_byte_len(start, offset, x) }
}
