
# Improvement Notes and Todo Items

This is a list of things I intend to do as a brief roadmap

(Note: Windows support is out of scope for pre-1.0 releases)

## 0. Project Naming

The current name needs reconsideration. It was originally created as a joke that went a bit too far, and now we're rather stuck with it.

## 1. macOS Performance Investigations

NOTE: Optimisations for other platforms such as BSD are out of scope due to obscure use case.

Despite attempting several approaches for macOS optimisation via `getattrlistbulk` and `getdirentries64` through raw syscalls (commented code can be found in `src/utils.rs` - search for GETDIRENTRIES).

macOS appears to skip this call in certain circumstances, possibly on empty directories. Without available source code, system call tracing is the only option (use `dtruss` for this purpose).

Whilst `getattrlistbulk` is only faster when requesting extended attributes, the behaviour of the latter approach remains unclear.

A potential alternative is [`fts_open`](https://blog.tempel.org/2019/04/dir-read-performance.html).

However, this presents a rather unpleasant API that is difficult to parallelise.

Also, look at this link for [`fts open`](https://github.com/dalance/fts-rs)

For investigation purposes, the last commit before removing `getdirentries64` can be found at:

```bash
git checkout 27728cdadcd254a95bda48a3f10b6c8d892bea0d
```

## 2. Filesystem Optimisations

- Implement a more efficient method to exclude ReiserFS - essentially a one-time assertion that isn't repeatedly called
- Consider improved sorting algorithms in `src/printer.rs`

- Consider using `statx` on Linux, though this presents some challenges due to `statx` only recently becoming available on MUSL. However, `statx` only requests the attributes explicitly asked for, potentially offering speed benefits as well as additional metadata. This requires careful consideration. See the [Rust implementation](https://github.com/rust-lang/rust/blob/07bdbaedc63094281483c40a88a1a8f2f8ffadc5/library/std/src/sys/fs/unix.rs#L105) for reference.

## 3. Iterator Enhancements

An alternative iterator implementation: when `stat` calls are known to be required in advance, execute them within the iterator itself.

This presents challenges as `stat` is a large structure. It could be stored as `Option<Cell<Box<stat>>>`, keeping the `direntry` structure under 64 bytes to fit within a single cache line.

Note: This must use `stat` type calls, not `lstat` type ones. Consider using `fstatat` with `AT_SYMLINK_NOFOLLOW` as an alternative approach.

EDIT:

After doing some research

(quick benchmarks, roughly done)

It seems fstat calls are IMMENSELY faster than stat.

This is my next priority to address! Surprisingly, statx doesn't seem to have any benefits speedwise, even seems to be slower!

Personally, I don't want to chase down statx anyway, because it means linux/musl/other Posix systems will have another separate code layer that will add even MORE complexity. Not advised!

---

fstat                   time:   [126.25 ns 126.40 ns 126.57 ns]
                        change: [−23.349% −22.681% −22.082%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 14 outliers among 100 measurements (14.00%)
  3 (3.00%) high mild
  11 (11.00%) high severe

stat                    time:   [274.11 ns 274.87 ns 275.72 ns]
                        change: [−24.565% −23.430% −22.442%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 3 outliers among 100 measurements (3.00%)
  3 (3.00%) high mild

---

## 4. Additional Features

Implement extra functionality such as custom `.fdfignore` files. This shouldn't be particularly difficult to implement. Additional features to be added as requirements arise.

**Note**: Ideally implemented after the parallelisation restructure below is resolved.

## 5. Parallelisation Restructure

Restructure the parallelisation approach in `src/lib.rs`. Whilst regex patterns can be shared between threads (they maintain an internal thread pool), this is likely inefficient.

A workaround exists, interestingly documented in [this GitHub issue](https://github.com/rust-lang/regex/issues/934):

This might be fixed due to this change however [see link](https://github.com/rust-lang/regex/issues/934#issuecomment-1703860708)

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

## 7. CIFS Filesystem Issue

The `getdents` skip code (in `src/iter.rs` around line 243) encounters issues on certain exotic CIFS filesystems. This was observed on a friend's server setup but hasn't been reproducible since, and access to the original server is no longer available.

Reverting to the standard `getdents` "call until 0" paradigm resolved the issue and returned the expected results.

However, I want to keep the syscall skip, I suspect it may be something to do with the block size.

## 8. Performance Profiling

Comprehensive performance profiling across different filesystem types and usage patterns to identify bottlenecks and optimisation opportunities.

Experiments such as playing with buffer sizes(linux/android), found in [script here](./scripts/test_buffer_sizes.sh)

Others exist, this will be added as time goes on.

TODO: Experiment in printer.rs with passing vectors in printer.rs to another thread (investigate this)

TODO: Investigate different allocators in macos

## 10. Modularisation (because 7 8 9 )

I wish to follow up with splitting the crate into a cli and internals, with the internals being UNIX/macOS/Linux/Windows
Generally just for macOS/Linux specialisations, no need to specialise for all OS'es. WAY too much work and no guarantee of result!
