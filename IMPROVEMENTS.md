
# Improvement Notes and Todo Items

(Note: Windows support is out of scope for pre-1.0 releases)

## 0. Project Naming

The current name needs reconsideration. It was originally created as a joke that went a bit too far, and now we're rather stuck with it.

## 1. macOS Performance Investigations

Despite attempting several approaches for macOS optimisation via `getattrlistbulk` and `getdirentries64` through raw syscalls (commented code can be found in `src/utils.rs` - search for GETDIRENTRIES).

macOS appears to skip this call in certain circumstances, possibly on empty directories. Without available source code, system call tracing is the only option (use `dtruss` for this purpose).

Whilst `getattrlistbulk` is only faster when requesting extended attributes, the behaviour of the latter approach remains unclear.

A potential alternative is [`fts_open`](https://blog.tempel.org/2019/04/dir-read-performance.html).

However, this presents a rather unpleasant API that is difficult to parallelise.

For investigation purposes, the last commit before removing `getdirentries64` can be found at:

Also, look at this link for [`fts open`](https://github.com/dalance/fts-rs)

```bash
git checkout 27728cdadcd254a95bda48a3f10b6c8d892bea0d
```

## 2. ZFS Handling

This addresses issues with large filename length edge cases. The relevant implementation is in `build.rs` and `src/iter.rs` (approximately line 250).

An elegant solution is needed that doesn't require rebuilding - perhaps using `LazyLock` without paying initialisation costs. This could be achieved with `OnceCell` or `std::cell::LazyCell`. Thread safety isn't a concern as this is effectively constant.

There's currently a fundamental flaw: building without ZFS support and subsequently installing ZFS (or ReiserFS) works fine. The challenge is implementing a very low-cost runtime check.

## 3. Filesystem Optimisations

- Implement a more efficient method to exclude ReiserFS - essentially a one-time assertion that isn't repeatedly called
- Consider improved sorting algorithms in `src/printer.rs`

- Consider using `statx` on Linux, though this presents some challenges due to `statx` only recently becoming available on MUSL. However, `statx` only requests the attributes explicitly asked for, potentially offering speed benefits as well as additional metadata. This requires careful consideration. See the [Rust implementation](https://github.com/rust-lang/rust/blob/07bdbaedc63094281483c40a88a1a8f2f8ffadc5/library/std/src/sys/fs/unix.rs#L105) for reference.

## 4. Iterator Enhancements

An alternative iterator implementation: when `stat` calls are known to be required in advance, execute them within the iterator itself.

This presents challenges as `stat` is a large structure. It could be stored as `Option<Cell<stat>>`, keeping the `direntry` structure under 64 bytes to fit within a single cache line.

Note: This must use `stat`, not `lstat`. Consider using `fstatat` with `AT_SYMLINK_NOFOLLOW` as an alternative approach.

## 5. Additional Features

Implement extra functionality such as custom `.fdfignore` files. This shouldn't be particularly difficult to implement. Additional features to be added as requirements arise.

**Note**: Ideally implemented after the parallelisation restructure below is resolved.

## 6. Parallelisation Restructure

Restructure the parallelisation approach in `src/lib.rs`. Whilst regex patterns can be shared between threads (they maintain an internal thread pool), this is likely inefficient.

A workaround exists, interestingly documented in [this GitHub issue](https://github.com/rust-lang/regex/issues/934):

```rust
struct TLSRegex {
    base: regex::bytes::Regex,
    local: thread_local::ThreadLocal<regex::bytes::Regex>,
}

impl TLSRegex {
    pub fn is_match(&self, path: &[u8]) -> bool {
        self.local.get_or(|| self.base.clone()).is_match(path)
    }
}
```

## 7. Future Considerations

Additional improvements to be determined based on ongoing development needs.

## 8. CIFS Filesystem Issue

The `getdents` skip code (in `src/iter.rs` around line 243) encounters issues on certain exotic CIFS filesystems. This was observed on a friend's server setup but hasn't been reproducible since, and access to the original server is no longer available.

Reverting to the standard `getdents` "call until 0" paradigm resolved the issue and returned the expected results.

## 9. Performance Profiling

Comprehensive performance profiling across different filesystem types and usage patterns to identify bottlenecks and optimisation opportunities.
