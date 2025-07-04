
#![allow(dead_code)]



include!(concat!(env!("OUT_DIR"), "/ls_colors.rs"));






#[inline]
pub fn colour_path_or_alternative<'a>(extension:&'a [u8],or_alternative:&'a [u8])->&'a [u8]{
    LS_COLOURS_HASHMAP.get(extension).map(|v| &**v).unwrap_or_else(||or_alternative)
}
