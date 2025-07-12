#![allow(clippy::host_endian_bytes)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::items_after_statements)]
// I was reading through the std library for random silly things and I found this , https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161
// this essentially provides a more rigorous foundation to my SWAR technique.
//the original definition is below the copy pasted code above.

//I really prefer having some strong foundation to rely on, so I'll use it and say stuff it to pride. Make it easy for people to verify.

//copy pasting code here, will probably add something in the readme about it.
//
//I have not (yet, this comment maybe wrong)
// I might do it, depends on use case.
//ive rewritten memchr to not rely on nightly too, so i can use without any deps

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

/*

#[cfg(target_os = "linux")]
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1; //we're going backwards(to the start of d_name) so we add 1 to the offset
    let reclen = unsafe { (*dirent).d_reclen } as usize; //(do not access it via byte_offset!)
    //let reclen_new=unsafe{ const {(*dirent).d_reclen}}; //reclen is the length of the dirent structure, including the d_name field
    // Calculate find the  start of the d_name field
    //  Access the last 8 bytes(word) of the dirent structure as a u64 word
    #[cfg(target_endian = "little")]
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }; //DO NOT USE BYTE OFFSET.
    #[cfg(target_endian = "big")]
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }.to_le(); // Convert to little-endian if necessary
    // Special case: When processing the 3rd u64 word (index 2), we need to mask
    // the non-name bytes (d_type and padding) to avoid false null detection.
    //  Access the last 8 bytes(word) of the dirent structure as a u64 word
    // The 0x00FF_FFFF mask preserves only the 3 bytes where the name could start.
    // Branchless masking: avoids branching by using a mask that is either 0 or 0x00FF_FFFF
    // Special handling for 24-byte records (common case):
    // Mask out non-name bytes (d_type and padding) that could cause false null detection
    let mask = 0x00FF_FFFFu64 * ((reclen == 24) as u64); // (multiply by 0 or 1)
    // The mask is applied to the last word to isolate the relevant bytes.
    // The last word is masked to isolate the relevant bytes,
    //we're bit manipulating the last word (a byte/u64) to find the first null byte
    //this boils to a complexity of strlen over 8 bytes, which we then accomplish with a bit trick
    // Combine the word with our mask to ensure:
    // - Original name bytes remain unchanged
    // - Non-name bytes are set to 0xFF (guaranteed non-zero)
    let candidate_pos = last_word | mask;
    // The resulting value (`candidate_pos`) has:
    // - Original name bytes preserved
    // - Non-name bytes forced to 0xFF (guaranteed non-zero)
    // - Maintains the exact position of any null bytes in the name
    //  Subtract 0x0101... from each byte (underflows if byte was 0)
    //  AND with inverse to isolate underflowed bits
    //  Mask high bits to find first zero byte
    let zero_bit = candidate_pos.wrapping_sub(0x0101_0101_0101_0101)// 0x0101_0101_0101_0101 -> underflows the high bit if a byte is zero
        & !candidate_pos//ensures only bytes that were zero retain the underflowed high bit.
        & 0x8080_8080_8080_8080; //  0x8080_8080_8080_8080 -->This masks out the high bit of each byte, so we can find the first zero byte
    // The trailing zeros of the zero_bit gives us the position of the first zero byte.
    // We divide by 8 to convert the bit position to a byte position..
    // We subtract 7 to get the correct offset in the d_name field.
    //>> 3 converts from bit position to byte index (divides by 8)
    let byte_pos = 7 - (zero_bit.trailing_zeros() >> 3) as usize;
    // The final length is calculated as:
    // `reclen - DIRENT_HEADER_START - byte_pos`
    // This gives us the length of the d_name field, excluding the header and the null
    // byte position.
    reclen - DIRENT_HEADER_START - byte_pos
}

*/

pub(crate) const fn repeat_u8(x: u8) -> usize {
    usize::from_ne_bytes([x; size_of::<usize>()])
}

pub(crate) const fn repeat_u64(byte: u8) -> u64 {
    u64::from_ne_bytes([byte; size_of::<u64>()])
}

pub(crate) const LO_USIZE: usize = repeat_u8(0x01);

pub(crate) const HI_USIZE: usize = repeat_u8(0x80);
pub(crate) const LO_U64: u64 = repeat_u64(0x01);

pub(crate) const HI_U64: u64 = repeat_u64(0x80);

/// Returns the index (0..=7) of the first zero byte in a `u64` word.
///
///
/// the u64 needs to be properly aligned and in proper platform endian specific  format.
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
    //lsb is at 0 regaredless of endianness
    (zero_bit.trailing_zeros() >> 3) as usize
    //>> 3 converts from bit position to byte index (divides by 8)
}

/// Returns `true` if `x` contains any zero byte.
///
///  
/// COPY PASTED FROM STDLIB INTERNALS.
///
/// From *Matters Computational*, J. Arndt:
///
/// "The idea is to subtract one from each of the bytes and then look for
/// bytes where the borrow propagated all the way to the most significant
/// bit."
#[inline]
pub const fn contains_zero_byte(x: usize) -> bool {
    x.wrapping_sub(LO_USIZE) & !x & HI_USIZE != 0
}

/// Returns the last index matching the byte `x` in `text`.
///
///
///
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
