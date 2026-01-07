#![allow(clippy::host_endian_bytes)]
#![allow(clippy::multiple_unsafe_ops_per_block)]
#![allow(clippy::undocumented_unsafe_blocks)]
#![allow(clippy::empty_line_after_doc_comments)]

// I was reading through the std library for random silly things and I found this , https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161
// this essentially provides a more rigorous foundation to my SWAR technique.
//the original definition is below the copy pasted code above.
//#![allow(clippy::all)]
//#![allow(warnings)] //the warnings are from memchr and thats a std lib func, too strict lints!
//I really prefer having some strong foundation to rely on, so I'll use it and say stuff it to pride. Make it easy for people to verify.

///copy pasting code here, will probably add something in the readme about it.
///
///I have not (yet, this comment maybe wrong)
/// I might do it, depends on use case.
// ive rewritten memchr to not rely on nightly too, so i can use without any deps

/*


// Original implementation taken from rust-memchr.

// Copyright 2015 Andrew Gallant, bluss and Nicolas Koch


use crate::intrinsics::const_eval_select;


const LO_USIZE: usize = usize::repeat_u8(0x01);

const HI_USIZE: usize = usize::repeat_u8(0x80);

const USIZE_BYTES: usize = size_of::<usize>();


/// Returns `true` if `x` contains any zero byte.

///

/// From *Matters Computational*, J. Arndt:

///

/// "The idea is to subtract one from each of the bytes and then look for

/// bytes where the borrow propagated all the way to the most significant

/// bit."

#[inline]

const fn contains_zero_byte(x: usize) -> bool {

    x.wrapping_sub(LO_USIZE) & !x & HI_USIZE != 0

}


/// Returns the first index matching the byte `x` in `text`.

#[inline]

#[must_use]

pub const fn memchr(x: u8, text: &[u8]) -> Option<usize> {

    // Fast path for small slices.

    if text.len() < 2 * USIZE_BYTES {

        return memchr_naive(x, text);

    }


    memchr_aligned(x, text)

}


#[inline]

const fn memchr_naive(x: u8, text: &[u8]) -> Option<usize> {

    let mut i = 0;


    // FIXME(const-hack): Replace with `text.iter().pos(|c| *c == x)`.

    while i < text.len() {

        if text[i] == x {

            return Some(i);

        }


        i += 1;

    }


    None

}


#[rustc_allow_const_fn_unstable(const_eval_select)] // fallback impl has same behavior

const fn memchr_aligned(x: u8, text: &[u8]) -> Option<usize> {

    // The runtime version behaves the same as the compiletime version, it's

    // just more optimized.

    const_eval_select!(

        @capture { x: u8, text: &[u8] } -> Option<usize>:

        if const {

            memchr_naive(x, text)

        } else {

            // Scan for a single byte value by reading two `usize` words at a time.

            //

            // Split `text` in three parts

            // - unaligned initial part, before the first word aligned address in text

            // - body, scan by 2 words at a time

            // - the last remaining part, < 2 word size


            // search up to an aligned boundary

            let len = text.len();

            let ptr = text.as_ptr();

            let mut offset = ptr.align_offset(USIZE_BYTES);


            if offset > 0 {

                offset = offset.min(len);

                let slice = &text[..offset];

                if let Some(index) = memchr_naive(x, slice) {

                    return Some(index);

                }

            }


            // search the body of the text

            let repeated_x = usize::repeat_u8(x);

            while offset <= len - 2 * USIZE_BYTES {

                // SAFETY: the while's predicate guarantees a distance of at least 2 * usize_bytes

                // between the offset and the end of the slice.

                unsafe {

                    let u = *(ptr.add(offset) as *const usize);

                    let v = *(ptr.add(offset + USIZE_BYTES) as *const usize);


                    // break if there is a matching byte

                    let zu = contains_zero_byte(u ^ repeated_x);

                    let zv = contains_zero_byte(v ^ repeated_x);

                    if zu || zv {

                        break;

                    }

                }

                offset += USIZE_BYTES * 2;

            }


            // Find the byte after the point the body loop stopped.


            let slice =

            // SAFETY: offset is within bounds

                unsafe { super::from_raw_parts(text.as_ptr().add(offset), text.len() - offset) };

            if let Some(i) = memchr_naive(x, slice) { Some(offset + i) } else { None }

        }

    )

}


/// Returns the last index matching the byte `x` in `text`.

#[must_use]

pub fn memrchr(x: u8, text: &[u8]) -> Option<usize> {

    // Scan for a single byte value by reading two `usize` words at a time.

    //

    // Split `text` in three parts:

    // - unaligned tail, after the last word aligned address in text,

    // - body, scanned by 2 words at a time,

    // - the first remaining bytes, < 2 word size.

    let len = text.len();

    let ptr = text.as_ptr();

    type Chunk = usize;


    let (min_aligned_offset, max_aligned_offset) = {

        // We call this just to obtain the length of the prefix and suffix.

        // In the middle we always process two chunks at once.

        // SAFETY: transmuting `[u8]` to `[usize]` is safe except for size differences

        // which are handled by `align_to`.

        let (prefix, _, suffix) = unsafe { text.align_to::<(Chunk, Chunk)>() };

        (prefix.len(), len - suffix.len())

    };


    let mut offset = max_aligned_offset;

    if let Some(index) = text[offset..].iter().rposition(|elt| *elt == x) {

        return Some(offset + index);

    }


    // Search the body of the text, make sure we don't cross min_aligned_offset.

    // offset is always aligned, so just testing `>` is sufficient and avoids possible

    // overflow.

    let repeated_x = usize::repeat_u8(x);

    let chunk_bytes = size_of::<Chunk>();


    while offset > min_aligned_offset {

        // SAFETY: offset starts at len - suffix.len(), as long as it is greater than

        // min_aligned_offset (prefix.len()) the remaining distance is at least 2 * chunk_bytes.

        unsafe {

            let u = *(ptr.add(offset - 2 * chunk_bytes) as *const Chunk);

            let v = *(ptr.add(offset - chunk_bytes) as *const Chunk);


            // Break if there is a matching byte.

            let zu = contains_zero_byte(u ^ repeated_x);

            let zv = contains_zero_byte(v ^ repeated_x);

            if zu || zv {

                break;

            }

        }

        offset -= 2 * chunk_bytes;

    }


    // Find the byte before the point the body loop stopped.

    text[..offset].iter().rposition(|elt| *elt == x)

}


*/
use core::num::NonZeroU64;
use core::num::NonZeroUsize;
#[inline]
const fn repeat_u8(x: u8) -> usize {
    usize::from_ne_bytes([x; size_of::<usize>()])
}

// simplifying macro
macro_rules! find_swar_index {
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

// simplifying macro
macro_rules! find_swar_last_index {
    // SWAR
    ($num:expr) => {{
        #[cfg(target_endian = "big")]
        {
            // `$num` has the high bit (0x80) set in each byte that matched.
            // On big-endian, the last byte index corresponds to the least-significant
            // set bit in the word.
            (((usize::BITS - 1) - $num.trailing_zeros()) >> 3) as usize
        }
        #[cfg(target_endian = "little")]
        {
            // `$num` has the high bit (0x80) set in each byte that matched.
            // On little-endian, the last byte index corresponds to the most-significant
            // set bit in the word.
            (((usize::BITS - 1) - $num.leading_zeros()) >> 3) as usize
        }
    }};
}

#[inline]
const fn repeat_u64(byte: u8) -> u64 {
    u64::from_ne_bytes([byte; size_of::<u64>()])
}

const LO_USIZE: usize = repeat_u8(0x01);

const HI_USIZE: usize = repeat_u8(0x80);
const LO_U64: u64 = repeat_u64(0x01);

const HI_U64: u64 = repeat_u64(0x80);

/**
 Returns the index (0–7) of the first zero byte in a `u64` word.

 This function uses a **branchless bitwise method** to detect zero bytes
 efficiently, avoiding per-byte comparisons.

 **How it works:**
 - `x.wrapping_sub(LO_U64)` subtracts 1 from each byte.
 - `& !x` clears bits that were set in `x`, leaving candidates for zero bytes.
 - `& HI_U64` isolates the high bit of each byte.

 The resulting value has the high bit set only in bytes that were zero in `x`.

 We then use either:
 - `trailing_zeros() >> 3` on little-endian systems, or
 - `leading_zeros() >> 3` on big-endian systems

  To convert the bit index of the first match into a byte index (dividing by 8).

 **Returns:**
 - `Some(index)` where `index` is the byte position (0–7) of the first zero byte
 - `None` if no zero byte is present
*/
#[inline]
#[must_use]
pub const fn find_zero_byte_u64(x: u64) -> Option<usize> {
    let matches = NonZeroU64::new(x.wrapping_sub(LO_U64) & !x & HI_U64);

    if let Some(nonzero_matches) = matches {
        return Some(find_swar_index!(nonzero_matches));
    }
    None
}

/**
 Finds the first occurrence of a byte in a 64-bit word.

 This uses a bitwise technique to locate the first instance of
 the target byte `c` in the 64-bit value `str`. The operation works by:

 1. `XORing` each byte with the target value (resulting in 0 for matches)
 2. Applying a zero-byte detection algorithm to find matches
 3. Converting the bit position to a byte index

 # The Computation
 - `str ^ repeat_u64(c)`: Creates a value where matching bytes become 0
 - `.wrapping_sub(LO_U64)`: Subtracts 1 from each byte (wrapping)
 - `& !xor_result`: Clears bits where the XOR result had 1s
 - `& HI_U64`: Isolates the high bit of each byte

 The resulting word will have high bits set only for bytes that matched `c`.


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
# Notes
- Returns the first occurrence if the byte appears multiple times
- Returns `None` if the byte is not found
- Works for any byte value (0-255)

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
    let swarred = NonZeroU64::new(xor_result.wrapping_sub(LO_U64) & !xor_result & HI_U64);
    /*
    If you're asking why `NonZeroU64`, check `dirent_const_time_strlen` for more info.
    https://doc.rust-lang.org/src/core/num/nonzero.rs.html#599
    https://doc.rust-lang.org/beta/std/intrinsics/fn.ctlz_nonzero.html
    https://doc.rust-lang.org/beta/std/intrinsics/fn.cttz_nonzero.html
    */

    match swarred {
        Some(valid) => Some(find_swar_index!(valid)),
        None => None,
    }
}

/**
 Finds the last occurrence of a byte in a 64-bit word.

 This uses a bitwise technique to locate the last instance of
 the target byte `c` in the 64-bit value `str`. The operation works by:

 1.  `XORing`  each byte with the target value (resulting in 0 for matches)
 2. Applying a zero-byte detection algorithm to find matches
 3. Converting the bit position to a byte index

 # The Computation
 - `str ^ repeat_u64(c)`: Creates a value where matching bytes become 0
 - `.wrapping_sub(LO_U64)`: Subtracts 1 from each byte (wrapping)
 - `& !xor_result`: Clears bits where the XOR result had 1s
 - `& HI_U64`: Isolates the high bit of each byte

 The resulting word will have high bits set only for bytes that matched `c`.


 # Examples
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
# Notes
- Returns the last occurrence if the byte appears multiple times
- Returns `None` if the byte is not found
- Works for any byte value (0-255)

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
    const MASK: u64 = repeat_u64(0x7F);

    let x = u64::from_ne_bytes(bytestr) ^ repeat_u64(c);
    let y = (x & MASK).wrapping_add(MASK);

    let swarred = NonZeroU64::new(!(y | x | MASK));

    match swarred {
        Some(num) => Some(find_swar_last_index!(num)),
        None => None,
    }
}

/** Returns `true` if `x` contains any zero byte.


 From *Matters Computational*, J. Arndt:


"The edea is to subtract one from each of the bytes and then look for

 bytes where the borrow propagated all the way to the most significant  bit."

 COPY PASTED FROM STDLIB INTERNALS.
*/
#[inline]
#[must_use]
pub const fn contains_zero_byte(x: usize) -> bool {
    x.wrapping_sub(LO_USIZE) & !x & HI_USIZE != 0
}

#[inline]
pub const fn contains_zero_byte_reversed(x: usize) -> Option<NonZeroUsize> {
    const MASK: usize = repeat_u8(0x7F);

    let y = (x & MASK).wrapping_add(MASK);
    NonZeroUsize::new(!(y | x | MASK))
}

/*

the rightmost 0-byte.
Figure 6-2 shows a branch-free procedure for this function. The idea is to convert each 0-byte to 0x80,
and each nonzero byte to 0x00, and then use number of leading zeros. This procedure executes in
eight instructions if the machine has the number of leading zeros and nor instructions. Some similar
tricks are described in [Lamp].
Figure 6-2 Find leftmost 0-byte, branch-free code.
int zbytel(unsigned x) {
unsigned y;
int n;
// Original byte: 00 80 other
y = (x & 0x7F7F7F7F) + 0x7F7F7F7F; // 7F 7F 1xxxxxxx
y = ~(y | x | 0x7F7F7F7F); // 80 00 00000000
n = nlz(y) >> 3; // n = 0 ... 4, 4 if x
return n; // has no 0-byte.
}
The position of the rightmost 0-byte is given by the number of trailing 0's in the final value of y
computed above, divided by 8 (with fraction discarded). Using the expression for computing the
number of trailing 0's by means of the number of leading zeros instruction (see Section 5- 4, "Counting
Trailing 0's," on page 84), this can be computed by replacing the assignment to n in the procedure
above with:
n = (32 - nlz(~y & (y - 1))) >> 3

*/

/// Returns the last index matching the byte `x` in `text`.
///
/// This is an optimised version of memrchr. As part of a commit towards stdlib.
#[must_use]
#[inline]
#[allow(clippy::cast_ptr_alignment)] //burntsushi wrote this so...
pub fn memrchr(x: u8, text: &[u8]) -> Option<usize> {
    // Scan for a single byte value by reading two `usize` words at a time.

    //

    // Split `text` in three parts:

    // - unaligned tail, after the last word aligned address in text,

    // - body, scanned by 2 words at a time,

    // - the first remaining bytes, < 2 word size.

    let len = text.len();

    let ptr = text.as_ptr();

    type Chunk = usize;

    let (min_aligned_offset, max_aligned_offset) = {
        // We call this just to obtain the length of the prefix and suffix.

        // In the middle we always process two chunks at once.

        // SAFETY: transmuting `[u8]` to `[usize]` is safe except for size differences

        // which are handled by `align_to`.

        let (prefix, _, suffix) = unsafe { text.align_to::<(Chunk, Chunk)>() };

        (prefix.len(), len - suffix.len())
    };

    let mut offset = max_aligned_offset;

    // compiler can't elide bounds checks on this.
    //if let Some(index) = text[offset..].iter().rposition(|elt| *elt == x)
    if let Some(index) = unsafe {
        text.get_unchecked(offset..)
            .iter()
            .rposition(|elt| *elt == x)
    } {
        return Some(offset + index);
    }

    // Search the body of the text, make sure we don't cross min_aligned_offset.

    // offset is always aligned, so just testing `>` is sufficient and avoids possible

    // overflow.

    let repeated_x = repeat_u8(x);

    const CHUNK_BYTES: usize = size_of::<Chunk>();

    while offset > min_aligned_offset {
        // SAFETY: offset starts at len - suffix.len(), as long as it is greater than
        // min_aligned_offset (prefix.len()) the remaining distance is at least 2 * chunk_bytes.
        unsafe {
            let u = ptr.add(offset - 2 * CHUNK_BYTES).cast::<usize>().read();

            let v = ptr.add(offset - CHUNK_BYTES).cast::<usize>().read();

            // Break if there is a matching byte.
            // **CHECK UPPER FIRST**
            if let Some(upper) = contains_zero_byte_reversed(v ^ repeated_x) {
                let zero_byte_pos = find_swar_last_index!(upper);
                return Some(offset - CHUNK_BYTES + zero_byte_pos);
            }

            if let Some(lower) = contains_zero_byte_reversed(u ^ repeated_x) {
                let zero_byte_pos = find_swar_last_index!(lower);

                return Some(offset - 2 * CHUNK_BYTES + zero_byte_pos);
            }
        }

        offset -= 2 * CHUNK_BYTES;
    }

    // Find the byte before the point the body loop stopped.
    unsafe {
        text.get_unchecked(..offset)
            .iter()
            .rposition(|elt| *elt == x)
    }
    // text[..offset].iter().rposition(|elt| *elt == x), avoid a bounds check
    // I checked the assembly and it inserted panic branches, didn't like it (since this is panic free)
}
