// I was reading through the std library for random silly things and I found this , https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161
// this essentially provides a more rigorous foundation to my SWAR technique.

// code taken from https://github.com/gituser12981u2/memchr_stuff/blob/main/src/memchr_new.rs (my own work with a friend)
/*

READ

I was basically using this as a learning project, to do cool things, then I found an optimisation for memrchr that was nice

this code is a bit janky, not really important.

memrchr is significantly changed from stdlib implementation to use a more efficient swar method.

*/

use core::num::NonZero;
const USIZE_BYTES: usize = size_of::<usize>();
const LO_USIZE: usize = usize::MAX.div_euclid(0xFF); //avoid a lint check, divides perfectly., gives constant 0x0101 etc.
const HI_USIZE: usize = LO_USIZE * 0x80;
/// <https://github.com/virtyaluk/Bit-Twiddling-Hacks#determine-if-a-word-has-a-zero-byte>
macro_rules! mycroft {
    ($num:expr) => {
        ($num.wrapping_sub(LO_USIZE) & HI_USIZE & !$num)
    };
}

// I am simply too lazy to comment all of these, it turns out a nice optimisation existed for memrchr
// I have done so, it seems the same optimisation is available for memchr but I need to work on the details
// Once done, I'll add it to the stdlib as a PR potentially
// https://github.com/gituser12981u2/memchr_stuff/blob/big_endian_fix/src/memchr_new.rs

#[inline]
#[must_use]
const fn contains_zero_byte_borrow_fix(input: usize) -> Option<NonZero<usize>> {
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
    let mut classic = mycroft!(input);
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
    classic &= !(input << 7);
    // nvm scratch that, gives better assembly with this other expression, weird!!!!!!!!
    // classic &= !input << 7;
    /*
    SAFETY: `classic != 0` implies there is at least one real zero byte
    somewhere in the word (false positives only occur alongside a real zero
    due to borrow propagation), so `zero_mask` must be non-zero.
    Use this to get smarter intrinsic (aka ctlz/cttz non_zero)
    Note: Debug assertions(in std) check zero_mask!=0 so check tests for comprehensive validation
    */
    Some(unsafe { NonZero::new_unchecked(classic) })
}

// This is an optimised version of memrchr

/// Returns the last index matching the byte `x` in `text`.
///
///# References
///- [Stanford Bit Twiddling Hacks find 0 byte ](http://www.icodeguru.com/Embedded/Hacker%27s-Delight/043.htm)
///- [Original memrchr implementation ](https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161)
#[must_use]
#[inline]
#[track_caller]
#[expect(clippy::indexing_slicing, reason = "panic free")]
pub fn memrchr(x: u8, text: &[u8]) -> Option<usize> {
    const SIZE: usize = 2 * USIZE_BYTES;
    const { assert!(SIZE.is_multiple_of(USIZE_BYTES), "trivial") }
    // Scan for a single byte value by reading two `usize` words at a time.
    // Split `text` in three parts:
    // - unaligned tail, after the last word aligned address in text,
    // - body, scanned by 2 words at a time,
    // - the first remaining bytes, < 2 word size.

    let slen = text.len();
    let start = text.as_ptr();
    // Number of bytes to reach the next `2*usize` boundary.
    let prefix = start.align_offset(SIZE);

    // First 2-register-aligned offset, clamped for short inputs.
    let aligned_start = prefix.min(slen);

    // End of the largest 2-register-aligned region,slen>=aligned_start, never wraps
    let aligned_end = aligned_start + ((slen - aligned_start) & const { !(SIZE - 1) });
    // Simple scalar byte scan lambda, avoid iterator overhead.(Reverse iterators optimise quite badly plus we're specialising this by
    //specialising this, because we use the base_ptr and do basic pointer arithmetic, we can just already use existing registers(holding the base_ptr)
    // SAFETY: trivial, allows compiler to elide panic branch, this is always true.
    unsafe { core::hint::assert_unchecked(aligned_end <= slen) };

    // check the tail for the value
    if let Some(byte_ptr) = text[aligned_end..].iter().rfind(|b| **b == x) {
        // the address of byte_ptr is always greater than start, finding this distance is exactly `x`'s position
        // find the address of the pointer that dereferenced to x, then get distance from start pointer
        return Some((&raw const *byte_ptr).addr() - start.addr()); //optimises better than rposition.
    }

    let mut offset = aligned_end;

    let repeated_x = (x as usize) * LO_USIZE; //broadcast x to all 4/2 nibble positions (or 1 if 16bit is ever possible)

    // define simple lambda  to avoid repetitive code.
    let find_last = |offset_: usize| -> Option<u32> {
        //avoid aliasing with offset
        // ptr add,cast the upper/lower to a usize, we did all the math to make sure we're only reading aligned chunks,
        // we could simplify this with a deref but I want the code to be specifically indicating aigned reads (pedantic aside, just taste)
        // SAFETY: aligned+inbounds; add offset,read the pointer and XOR mask to set matching bits to 0 (and 0's to x)
        let upper_or_lower = unsafe { start.add(offset_).cast::<usize>().read() } ^ repeated_x;
        const BITSMINUS1: u32 = usize::BITS - 1;
        if cfg!(target_endian = "big") {
            // BE: the last byte in memory is the least significant one,
            // so the "last match" is the lowest set bit
            NonZero::new(mycroft!(upper_or_lower)).map(|y| (BITSMINUS1 - y.trailing_zeros()) >> 3)
        } else {
            // LE: the last byte in memory is the most significant one,
            // so the "last match" is the highest set bit
            contains_zero_byte_borrow_fix(upper_or_lower)
                .map(|y| (BITSMINUS1 - y.leading_zeros()) >> 3)
        }
    };
    /*
    Search the body of the text, make sure we don't cross aligned_start.
    offset is always aligned, so just testing `>` is sufficient and avoids possible overflow.
    NBD: using  = optimises worse but is equivalent.
    */
    // this is formulated in a very obtuse way, sorry, massaging assembly.
    if let Some(zero_byte_pos) = 'search: {
        while offset > aligned_start {
            offset -= USIZE_BYTES;
            if let hit @ Some(_) = find_last(offset) {
                break 'search hit;
            }
            offset -= USIZE_BYTES;
            if let hit @ Some(_) = find_last(offset) {
                break 'search hit;
            }
        }
        None
    } {
        // add offset to the zero byte bos in THIS part, not in the loop!
        return Some(offset + zero_byte_pos as usize);
    }

    debug_assert!(offset == aligned_start, "test"); // they're always equal, but llvm optimisesr better using offset instead of aligned start because it repurposes a register
    // The character we were looking for didn't appear in the aligned body, do a simple loop to check the head segment.
    text[..offset].iter().rposition(|b| *b == x)

    // this is much simpler because head=text[..offset] so we're only reading the very start of the slice, no shenanigans needed!
}
