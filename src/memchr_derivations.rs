// I was reading through the std library for random silly things and I found this , https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161
// this essentially provides a more rigorous foundation to my SWAR technique.
//the original definition is below the copy pasted code above.
#![allow(clippy::all)]
#![allow(warnings)]
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

            // FIXME(const-hack): Use `?` instead.

            // FIXME(const-hack, fee1-dead): use range slicing

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





#[inline]
pub(crate) const fn repeat_u8(x: u8) -> usize {
    usize::from_ne_bytes([x; size_of::<usize>()])
}

#[inline]
pub(crate) const fn repeat_u64(byte: u8) -> u64 {
    u64::from_ne_bytes([byte; size_of::<u64>()])
}

const LO_USIZE: usize = repeat_u8(0x01);

const HI_USIZE: usize = repeat_u8(0x80);
const LO_U64: u64 = repeat_u64(0x01);

const HI_U64: u64 = repeat_u64(0x80);

/// Returns the index (0..=7) of the first zero byte** in a `u64` word.
/// IT MUST CONTAIN A NULL TERMINATOR
///
/// This uses a branchless, bitwise technique that identifies zero bytes
/// by subtracting `0x01` from each byte and masking out non-zero bytes.
///
///
/// The computation:
/// - `x.wrapping_sub(LO_U64)`: subtracts 1 from each byte
/// - `& !x`: clears bits where x had 1s (preserves potential zero bytes)
/// - `& HI_U64`: isolates the high bit of each byte
///
/// The resulting word will have high bits set only for zero bytes in `x`.
/// We then use `trailing_zeros() >> 3` to get the byte index (0-based).
///
/// Returns:
/// - The byte index of the first zero byte in `x`
#[inline]
pub const fn find_zero_byte_u64(x: u64) -> usize {
    //use the same trick seen earlier, except this time we have to use  hardcoded u64 values  to find the position of the 0 bit
    let zero_bit = x.wrapping_sub(LO_U64) & !x & HI_U64;

    (zero_bit.trailing_zeros() >> 3) as usize
    //>> 3 converts from bit position to byte index (divides by 8)
}


#[inline]
/// Finds the first occurrence of a byte in a 64-bit word.
///
/// This uses a branchless, bitwise technique to locate the first instance of 
/// the target byte `c` in the 64-bit value `str`. The operation works by:
///
/// 1. XORing each byte with the target value (resulting in 0 for matches)
/// 2. Applying a zero-byte detection algorithm to find matches
/// 3. Converting the bit position to a byte index
///
/// # The Computation
/// - `str ^ repeat_u64(c)`: Creates a value where matching bytes become 0
/// - `.wrapping_sub(LO_U64)`: Subtracts 1 from each byte (wrapping)
/// - `& !xor_result`: Clears bits where the XOR result had 1s
/// - `& HI_U64`: Isolates the high bit of each byte
///
/// The resulting word will have high bits set only for bytes that matched `c`.
/// We then use `trailing_zeros() >> 3` to convert the bit position to a byte index.
///
/// # Examples
/// ```
/// use fdf::{find_char_in_word};
/// // Basic usage
/// assert_eq!(find_char_in_word(b'C', [b'A', b'B', b'C', b'D', 0, 0, 0, 0]), Some(2));
/// assert_eq!(find_char_in_word(b'X', [b'A', b'B', b'C', b'D', 0, 0, 0, 0]), None);
///
/// // Edge cases
/// assert_eq!(find_char_in_word(b'A', [b'A'; 8]), Some(0)); // first position
/// assert_eq!(find_char_in_word(b'A', [0; 8]), None); // not found
/// assert_eq!(find_char_in_word(0, [1, 2, 3, 0, 5, 6, 7, 8]), Some(3)); // null byte
/// ```
///
/// # Notes
/// - Returns the first occurrence if the byte appears multiple times
/// - Returns `None` if the byte is not found
/// - Works for any byte value (0-255)
///
/// # Parameters
/// - `c`: The byte to search for (0-255)
/// - `str`: The word ( a [u8;8] ) to search in (64 bit specific)
///
/// # Returns
/// - `Some(usize)`: Index (0-7) of the first occurrence
/// - `None`: If the byte is not found
pub const fn find_char_in_word(c: u8, str: [u8;8]) -> Option<usize> {
    // XOR with the target character will be 0 for matching bytes
    let char_array=u64::from_ne_bytes(str);
    let xor_result = char_array ^ repeat_u64(c);
    
    // Find zero bytes in the XOR result 
    let matches = (xor_result.wrapping_sub(LO_U64)) & !xor_result & HI_U64;
    
    if matches != 0 {
        Some((matches.trailing_zeros() >> 3) as usize)
    } else {
        None
    }
}

/// Returns `true` if `x` contains any zero byte.
///

/// From *Matters Computational*, J. Arndt:

///

/// "The idea is to subtract one from each of the bytes and then look for

/// bytes where the borrow propagated all the way to the most significant

/// bit."
///
/// COPY PASTED FROM STDLIB INTERNALS.

#[inline]
pub const fn contains_zero_byte(x: usize) -> bool {
    x.wrapping_sub(LO_USIZE) & !x & HI_USIZE != 0
}

/// Returns the last index matching the byte `x` in `text`.
/// This is directly copy pasted from the internal library with some modifications to make it work for me
/// there were no unstable features so I thought I'll skip a dependency and add this.
///
#[must_use]
#[inline]
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

    let repeated_x = repeat_u8(x);

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

/*

//these are now working in normal rust, but i havent used them in my crate, i think i will soon!

const USIZE_BYTES: usize = size_of::<usize>();



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







fn memchr_aligned(x: u8, text: &[u8]) -> Option<usize> {



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

            let repeated_x = repeat_u8(x);

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

            // FIXME(const-hack): Use `?` instead.

            // FIXME(const-hack, fee1-dead): use range slicing

            let slice =

            // SAFETY: offset is within bounds

                unsafe { &*std::ptr::slice_from_raw_parts(text.as_ptr().add(offset), text.len() - offset) };

            if let Some(i) = memchr_naive(x, slice) { Some(offset + i) } else { None }

        }

    */
