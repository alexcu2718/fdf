// simple toggleable tests

#[cfg(test)]
const DETERMINISTIC: bool = true;

#[cfg(test)]
const RANDOM_SEED: u64 = 4269;

#[cfg(test)]
mod tests {
    #![allow(dead_code)]
    #![allow(unused_imports)]
    use super::*;
    use crate::filters::{SizeFilter, TimeFilter};
    use crate::fs::{DirEntry, FileType};
    use crate::util::{BytePath, find_char_in_word, find_last_char_in_word};
    use crate::walk::Finder;
    use chrono::{Duration as ChronoDuration, Utc};
    use env_home::env_home_dir;
    use filetime::{FileTime, set_file_times};
    use std::env::temp_dir;
    use std::ffi::OsStr;
    use std::ffi::OsString;
    use std::fs;
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rand::rngs::StdRng;
    use rand::{Rng, RngExt, SeedableRng, rng};

    #[allow(dead_code)]
    #[repr(C)]
    pub struct Dirent64 {
        d_ino: u64,
        d_off: u64,
        d_reclen: u16,
        d_type: u8,
        d_name: [u8; 256], // from definition in linux, this is really hacked in order to show my fancy-schmancy SWAR works.
    }

    pub fn generate_random_byte_strings(
        count: usize,
        string_size: usize,
        deterministic: bool,
    ) -> Vec<Vec<u8>> {
        let mut rng: Box<dyn Rng> = if deterministic {
            Box::new(StdRng::seed_from_u64(RANDOM_SEED))
        } else {
            Box::new(rng())
        };

        let mut strings = Vec::with_capacity(count);

        for _ in 0..count {
            // random strings with varying lengths from 0 to MAX SIZED STRING
            let length = rng.random_range(0..=string_size);
            let bytes: Vec<u8> = (0..length).map(|_| rng.random()).collect();
            strings.push(bytes);
        }

        strings
    }

    pub fn generate_random_u64_arrays(count: usize, deterministic: bool) -> Vec<[u8; 8]> {
        let mut rng: Box<dyn Rng> = if deterministic {
            Box::new(StdRng::seed_from_u64(RANDOM_SEED))
        } else {
            Box::new(rng())
        };

        let mut arrays = Vec::with_capacity(count);
        for _ in 0..count {
            let mut bytes = [0u8; 8];
            rng.fill_bytes(&mut bytes);
            arrays.push(bytes);
        }

        arrays
    }

    fn create_byte_array(s: &str) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        let s_bytes = s.as_bytes();
        let len = s_bytes.len().min(8);
        bytes[..len].copy_from_slice(&s_bytes[..len]);
        bytes
    }

    #[test]
    fn tmemrchr() {
        let byte_strings = generate_random_byte_strings(1000, 1000, DETERMINISTIC);
        let random_chars = 0..=u8::MAX;

        for byte in random_chars {
            for string in &byte_strings {
                test_memrchr(byte, string);
            }
        }
    }

    #[test]
    fn test_reversed() {
        let arrays = generate_random_u64_arrays(1000, DETERMINISTIC);

        for bytes in arrays.iter() {
            for i in 0..=u8::MAX {
                let expected_pos = bytes.iter().rposition(|&b| b == i);

                let detected_pos = crate::util::find_last_char_in_word(i, *bytes);

                assert_eq!(
                    detected_pos,
                    expected_pos,
                    "Mismatch for word={:#018x} bytes={bytes:?} in contains last zero byte!",
                    u64::from_ne_bytes(*bytes)
                );
            }
        }
    }

    #[test]
    fn test_forward() {
        let arrays = generate_random_u64_arrays(1000, DETERMINISTIC);

        for bytes in arrays.iter() {
            for i in 0..=u8::MAX {
                let expected_pos = bytes.iter().position(|&b| b == i);

                let detected_pos = crate::util::find_char_in_word(i, *bytes);

                assert_eq!(
                    detected_pos,
                    expected_pos,
                    "Mismatch for word={:#018x} bytes={bytes:?} in contains last zero byte!",
                    u64::from_ne_bytes(*bytes)
                );
            }
        }
    }

    fn test_memrchr(search: u8, sl: &[u8]) {
        let realans = sl.iter().rposition(|b| *b == search);
        let memrchrtest = crate::util::memrchr(search, sl);
        assert!(
            memrchrtest == realans,
            "test failed in memrchr: expected {realans:?}, got {memrchrtest:?} for byte {search:#04x}\n
            searching for {} with ASCII value {search} in slice {}",
            char::from(search),String::from_utf8_lossy(sl)
        );
    }

    const fn dirent_reclen_for_name_len(name_len: usize) -> u16 {
        debug_assert!(name_len <= 255);
        let header_start = core::mem::offset_of!(Dirent64, d_name);
        // +1 for the required null terminator
        let min_len = header_start + name_len + 1;
        // `dirent_const_time_strlen` assumes 8-byte alignment / `d_reclen` multiples of 8.
        let reclen = min_len.next_multiple_of(8);
        debug_assert!(reclen <= u16::MAX as usize);
        reclen as u16
    }

    fn generate_random_dirents64(
        count: usize,
        max_name_len: usize,
        deterministic: bool,
    ) -> Vec<Dirent64> {
        let max_name_len = max_name_len.min(255);
        let random_names = generate_random_byte_strings(count, max_name_len, deterministic);

        let mut dirents = Vec::with_capacity(count);

        for raw in random_names {
            let name_len = raw.len().min(255);
            let mut entry = Dirent64 {
                d_ino: 0,
                d_off: 0,
                d_reclen: dirent_reclen_for_name_len(name_len),
                d_type: 0,
                d_name: [0; 256],
            };

            for (i, &b) in raw.iter().take(name_len).enumerate() {
                entry.d_name[i] = if b == 0 { 1 } else { b };
            }
            entry.d_name[name_len] = 0;

            dirents.push(entry);
        }

        dirents
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn test_dirent_const_time_strlen_randomised_dirents() {
        let dirents = generate_random_dirents64(2_000, 255, DETERMINISTIC);

        for entry in &dirents {
            let expected = entry
                .d_name
                .iter()
                .position(|&b| b == 0)
                .expect("generated d_name must contain a null terminator");

            let got = unsafe {
                crate::util::dirent_const_time_strlen(std::mem::transmute::<
                    *const Dirent64,
                    *const libc::dirent64,
                >(entry))
            };

            assert_eq!(
                got, expected,
                "dirent_const_time_strlen mismatch: got {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn check_filenames() {
        let temp_dir = std::env::temp_dir();
        let file_name = "parent_TEST.txt";
        let file_path = temp_dir.as_path().join(file_name);

        let _ = std::fs::File::create(&file_path);

        let entry = DirEntry::new(file_path.as_os_str()).unwrap();
        let _ = std::fs::remove_file(&file_path);
        assert_eq!(entry.file_name(), file_name.as_bytes());
    }

    #[test]
    fn test_directory_traversal_permissions() {
        let temp_dir = temp_dir().join("traversal_test_again");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        // no read permission
        let no_read_dir = temp_dir.join("no_read");
        let _ = fs::remove_dir_all(&no_read_dir);
        let _ = fs::create_dir(&no_read_dir);

        let mut perms = fs::metadata(&no_read_dir).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&no_read_dir, perms).unwrap();

        let entry = DirEntry::new(&temp_dir).unwrap();
        let iter = entry.readdir().unwrap();

        let entries: Vec<_> = iter.collect();

        let mut perms = fs::metadata(&no_read_dir).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&no_read_dir, perms).unwrap();
        //cleanup code

        fs::remove_dir_all(temp_dir).unwrap();
        assert!(entries.len() == 1)
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn test_directory_traversal_permissions_linux() {
        let temp_dir = temp_dir().join("traversal_test_again_linux");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        // no read permission
        let no_read_dir = temp_dir.join("no_read");
        let _ = fs::remove_dir_all(&no_read_dir);
        let _ = fs::create_dir(&no_read_dir);

        let mut perms = fs::metadata(&no_read_dir).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&no_read_dir, perms).unwrap();

        let entry = DirEntry::new(&temp_dir).unwrap();
        let iter = entry.getdents().unwrap();

        let entries: Vec<_> = iter.collect();

        let mut perms = fs::metadata(&no_read_dir).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&no_read_dir, perms).unwrap();
        //cleanup code

        fs::remove_dir_all(temp_dir).unwrap();
        assert!(entries.len() == 1)
    }

    #[test]
    fn test_permission_checks() {
        let temp_dir = temp_dir().join("perm_test");
        fs::create_dir_all(&temp_dir).unwrap();

        let entry = DirEntry::new(&temp_dir).unwrap();

        assert!(entry.exists());
        assert!(entry.is_readable());
        assert!(entry.is_writable());

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn modified_time_reflects_custom_timestamp() {
        let tmp_dir = temp_dir();
        let file_path = tmp_dir.join("modified_time_custom_test.txt");

        {
            let mut f = File::create(&file_path).expect("failed to create temp file");
            writeln!(f, "test contents").unwrap();
        }

        // Jan 1, 2000 UTC
        let custom_secs = 946684800; // seconds since epoch
        let custom_ft = FileTime::from_unix_time(custom_secs, 0);

        // Apply custom time
        set_file_times(&file_path, custom_ft, custom_ft).expect("failed to set file time");

        let dt = DirEntry::new(file_path.as_os_str())
            .unwrap()
            .modified_time()
            .expect("should return custom datetime");

        assert_eq!(
            dt.timestamp(),
            custom_secs,
            "Expected modified_time to equal custom timestamp"
        );

        fs::remove_file(file_path).ok();
    }

    #[test]
    fn modified_time_updates_after_file_touch() {
        let tmp_dir = temp_dir();
        let file_path = tmp_dir.join("modified_time_update_test.txt");

        {
            let mut f = File::create(&file_path).expect("failed to create temp file");
            writeln!(f, "initial contents").unwrap();
        }

        let first_time = DirEntry::new(file_path.as_os_str())
            .unwrap()
            .modified_time()
            .expect("should get initial modified time");

        // Sleep to ensure fs difference
        std::thread::sleep(std::time::Duration::from_secs(2));

        {
            let mut f = File::options()
                .append(true)
                .open(&file_path)
                .expect("failed to reopen temp file");
            writeln!(f, "new contents").unwrap();
        }

        let second_time = DirEntry::new(file_path.as_os_str())
            .unwrap()
            .modified_time()
            .expect("should get updated modified time");

        assert!(
            second_time > first_time,
            "Expected modified_time to increase after writing, but {:?} <= {:?}",
            second_time,
            first_time
        );

        fs::remove_file(file_path).ok();
    }
    #[test]
    fn test_iterating_nested_structure() {
        let dir = temp_dir().join("nested_struct");
        let _ = fs::remove_dir_all(&dir);
        let subdir = dir.join("sub");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "data").unwrap();

        let dir_entry = DirEntry::new(&dir).unwrap();
        let entries: Vec<_> = dir_entry.readdir().unwrap().collect();

        assert_eq!(entries.len(), 1, "Top-level should contain only subdir");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_size_filter_edge_zero_and_large() {
        let file_zero = temp_dir().join("zero_size.txt");
        File::create(&file_zero).unwrap();
        let entry = DirEntry::new(&file_zero).unwrap();
        let metadata = std::fs::metadata(entry).unwrap();
        assert_eq!(metadata.len(), 0);

        let filter = SizeFilter::Equals(0);
        assert!(filter.is_within_size(metadata.len()));

        let filter_large = SizeFilter::Min(10_000_000);
        assert!(!filter_large.is_within_size(metadata.len()));

        let _ = fs::remove_file(file_zero);
    }

    #[test]
    fn test_symlink_properties() {
        let dir = temp_dir().join("symlink_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let target = dir.join("target.txt");
        fs::write(&target, "data").unwrap();
        let link = dir.join("link.txt");
        let _ = symlink(&target, &link);

        let entry = DirEntry::new(&link).unwrap();
        assert!(entry.exists());
        assert!(entry.is_symlink());
        assert_eq!(entry.file_name(), b"link.txt");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn modified_time_returns_valid_datetime_for_file() {
        let tmp_dir = temp_dir();
        let file_path = tmp_dir.join("modified_time_test.txt");

        // Create file
        {
            let mut f = File::create(&file_path).expect("failed to create temp file");
            writeln!(f, "hello world").unwrap();
        }

        let dt = DirEntry::new(file_path.as_os_str())
            .unwrap()
            .modified_time()
            .expect("should return valid datetime");

        // Sanity check: timestamp should be close to now
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let dt_timestamp = dt.timestamp();
        assert!(
            (now - dt_timestamp).abs() < 5,
            "File modified_time too far from now: {dt_timestamp:?} vs {now:?}",
        );

        // cleanup
        fs::remove_file(file_path).ok();
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn test_getdents() {
        let temp_dir = std::env::temp_dir();
        let dir_path = temp_dir.as_path().join("testdir");
        let _ = std::fs::create_dir(&dir_path);
        //throwing the error incase it already exists the directory already exists

        std::fs::write(dir_path.join("file1.txt"), "test1").unwrap();
        std::fs::write(dir_path.join("file2.txt"), "test2").unwrap();
        let _ = std::fs::create_dir(dir_path.join("subdir")); //.unwrap();

        let dir_entry = DirEntry::new(dir_path.as_os_str()).unwrap();
        let entries = dir_entry.getdents().unwrap();
        let entries_clone: Vec<_> = dir_entry.getdents().unwrap().collect();

        let mut names: Vec<_> = entries.map(|e| e.file_name().to_vec()).collect();

        assert_eq!(entries_clone.len(), 3);

        names.sort();
        assert_eq!(
            names,
            vec![
                b"file1.txt".to_vec(),
                b"file2.txt".to_vec(),
                b"subdir".to_vec()
            ]
        );

        let entries_clone2: Vec<_> = dir_entry.getdents().unwrap().collect();

        let _ = std::fs::remove_dir_all(&dir_path);
        for entry in entries_clone2 {
            assert_eq!(entry.depth(), 1);
            assert_eq!(
                entry.file_name_index() as usize,
                dir_path.as_os_str().len() + 1
            );
        }

        //let _=std::fs::File::
    }

    #[test]
    fn test_read_dir() {
        let temp_dir = std::env::temp_dir();
        let dir_path = temp_dir.as_path().join("testdir");
        let _ = std::fs::create_dir(&dir_path);
        //throwing the error because who cares if the directory already exists

        std::fs::write(dir_path.join("file1.txt"), "test1").unwrap();
        std::fs::write(dir_path.join("file2.txt"), "test2").unwrap();
        let _ = std::fs::create_dir(dir_path.join("subdir")); //.unwrap();

        let dir_entry = DirEntry::new(dir_path.as_os_str()).unwrap();
        let entries = dir_entry.readdir().unwrap();
        let entries_clone: Vec<_> = dir_entry.readdir().unwrap().collect();

        let mut names: Vec<_> = entries.map(|e| e.file_name().to_vec()).collect();

        assert_eq!(entries_clone.len(), 3);

        names.sort();
        assert_eq!(
            names,
            vec![
                b"file1.txt".to_vec(),
                b"file2.txt".to_vec(),
                b"subdir".to_vec()
            ]
        );
        //yeah this test code is half arsed, but it's comprehensive,.
        let entries_clone2: Vec<_> = dir_entry.readdir().unwrap().collect();

        let _ = std::fs::remove_dir_all(&dir_path);
        for entry in entries_clone2 {
            assert_eq!(entry.depth(), 1);
            assert_eq!(entry.file_name_index(), dir_path.as_os_str().len() + 1);
        }

        //let _=std::fs::File::
    }

    #[test]
    fn test_hidden_files() {
        let dir_path = std::env::temp_dir().join("test_hidden");
        let _ = std::fs::create_dir_all(&dir_path);

        let _ = std::fs::File::create(dir_path.join("visible.txt"));
        let _ = std::fs::File::create(dir_path.join(".hidden"));

        let dir_entry = DirEntry::new(&dir_path).unwrap();
        let entries: Vec<_> = dir_entry.readdir().unwrap().collect();
        let mut names: Vec<_> = entries
            .iter()
            .map(|e| String::from_utf8_lossy(e.file_name()))
            .collect();
        names.sort();

        let _ = std::fs::remove_dir_all(&dir_path);
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], ".hidden");
        assert_eq!(names[1], "visible.txt");
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn test_hidden_files_linux_android() {
        let dir_path = std::env::temp_dir().join("test_hidden_linux_android");
        let _ = std::fs::remove_dir(&dir_path);
        let _ = std::fs::create_dir_all(&dir_path);

        let _ = std::fs::File::create(dir_path.join("visible.txt"));
        let _ = std::fs::File::create(dir_path.join(".hidden"));

        let dir_entry = DirEntry::new(&dir_path).unwrap();
        let entries: Vec<_> = dir_entry.getdents().unwrap().collect();
        let mut names: Vec<_> = entries
            .iter()
            .map(|e| String::from_utf8_lossy(e.file_name()))
            .collect();
        names.sort();

        let _ = std::fs::remove_dir_all(&dir_path);
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], ".hidden");
        assert_eq!(names[1], "visible.txt");
    }

    #[test]
    fn filename_test() {
        let temp_dir = std::env::temp_dir();
        let new_dir = temp_dir.as_path().join("testdir_filename");
        let _ = std::fs::remove_dir_all(&new_dir);
        let _ = std::fs::create_dir_all(&new_dir);
        let file_path = new_dir.join("testfile.txt");
        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::write(&file_path, "test");

        let entry = DirEntry::new(file_path.as_os_str()).unwrap();

        assert_eq!(entry.file_name(), b"testfile.txt");
        let x = std::fs::remove_file(&file_path).is_ok();
        assert!(x, "File should be removed successfully");
        let _ = std::fs::remove_dir_all(&new_dir);
    }
    #[test]
    fn base_len_test() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.as_path().join("testfilenew.txt");
        std::fs::write(&file_path, "test").unwrap();

        let entry: usize = DirEntry::new(file_path.as_os_str())
            .unwrap()
            .file_name_index();

        let std_entry: usize = (std::path::Path::new(file_path.as_os_str())
            .parent()
            .unwrap()
            .as_os_str()
            .len()
            + 1) as _;
        assert_eq!(entry, std_entry);

        let _ = std::fs::remove_file(&file_path);
    }
    #[test]
    fn test_full_path() {
        let temp_dir = std::env::temp_dir().join("test_full_path");

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);

        let _ = std::env::set_current_dir(&temp_dir); //.unwrap();

        let file_path = DirEntry::new(".")
            .unwrap()
            .to_full_path()
            .expect("should not fail");

        let my_path: Box<[u8]> = file_path.as_bytes().into();

        let my_path_std: std::path::PathBuf = std::path::Path::new(".")
            .canonicalize()
            .expect("should not fail");
        let bytes_std: &[u8] = my_path_std.as_os_str().as_bytes();
        assert_eq!(&*my_path, bytes_std);

        assert_eq!(file_path.is_dir(), my_path_std.is_dir());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg(not(target_os = "macos"))] //enable this test on macos and see why ive disabled it. **** stupid
    fn test_from_bytes() {
        //this is a mess of code but works lol to demonstrate infallibility(or idealllllllyyyyyyyyy...(ik its not))
        // Create a unique temp directory for this test
        let temp_dir = std::env::temp_dir().join("test_full_path_fdf");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Set up test file
        let test_file = temp_dir.join("test_file_fdf.txt");
        let _ = std::fs::write(&test_file, "test content");

        // Get canonical paths for comparison
        let test_file_canon = std::fs::canonicalize(&test_file).unwrap();
        let test_file_bytes = test_file_canon.as_os_str().as_bytes();

        // Test directory entry

        let dir_entry = DirEntry::new(&temp_dir)
            .expect("why did this fail? ")
            .to_full_path()
            .unwrap();

        let canonical_path = temp_dir.canonicalize().unwrap();

        // Compare paths at byte level
        let dir_bytes = dir_entry.as_os_str();
        let canonical_bytes = canonical_path.as_os_str();
        dbg!(&dir_bytes);
        dbg!(&canonical_bytes);

        // verify path conversions in bytes(i want to make sure every byte is right.)
        assert_eq!(
            dir_bytes.as_bytes(),
            canonical_bytes.as_bytes(),
            "Path bytes should match"
        );

        // iteration
        let mut entries = dir_entry.readdir().unwrap().into_iter().collect::<Vec<_>>();

        assert!(
            !entries.is_empty(),
            "Should find at least the directory itself"
        );

        let first_entry = entries.pop().unwrap();
        assert_eq!(
            first_entry.as_bytes(),
            test_file_bytes,
            "Directory entry should match canonical path"
        );

        //  file type detection

        let pathcheck = std::path::Path::new(first_entry.as_os_str())
            .canonicalize()
            .unwrap();

        assert!(pathcheck.is_file(), "should be a file");

        // Clean up
        let a = std::fs::remove_dir_all(temp_dir);
        assert!(a.is_ok(), "Should remove temp directory successfully");
    }

    #[test]
    fn test_iterator() {
        // make a unique test directory inside temp_dir
        let unique_id = "fdf_iterator_test";
        let dir_path: PathBuf = temp_dir().join(unique_id);
        let _ = fs::remove_dir_all(dir_path.as_path());
        let _ = fs::create_dir(dir_path.as_path());

        // create test files and subdirectory
        fs::write(dir_path.join("file1.txt"), "content").unwrap();
        fs::write(dir_path.join("file2.txt"), "content").unwrap();
        fs::create_dir(dir_path.join("subdir")).unwrap();

        // init a DirEntry for testing

        let dir_entry = DirEntry::new(&dir_path).unwrap();

        // get iterator
        let iter = dir_entry.readdir().unwrap();

        // collect entries
        let mut entries = Vec::new();

        for entry in iter {
            entries.push(entry)
        }

        //verify results

        //let _ = fs::remove_dir_all(dir_entry.as_path());
        assert_eq!(entries.len(), 3, "Should find two files and one subdir");

        assert!(
            entries.clone().iter().filter(|e| e.is_dir()).count() == 1,
            "Should find one directory"
        );
        assert!(
            entries
                .clone()
                .iter()
                .filter(|e| e.is_regular_file())
                .count()
                == 2,
            "Should find two regular files"
        );

        let _ = fs::remove_dir_all(dir_path.as_path());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_iterator_linux() {
        // make a unique test directory inside temp_dir
        let unique_id = "fdf_iterator_test_linux";
        let dir_path: PathBuf = temp_dir().join(unique_id);
        let _ = fs::remove_dir_all(dir_path.as_path());
        let _ = fs::create_dir(dir_path.as_path());

        // create test files and subdirectory
        fs::write(dir_path.join("file1.txt"), "content").unwrap();
        fs::write(dir_path.join("file2.txt"), "content").unwrap();
        fs::create_dir(dir_path.join("subdir")).unwrap();

        // init a DirEntry for testing

        let dir_entry = DirEntry::new(&dir_path).unwrap();

        // get iterator
        let iter = dir_entry.getdents().unwrap();

        // collect entries
        let mut entries = Vec::new();

        for entry in iter {
            entries.push(entry)
        }

        //verify results

        assert_eq!(entries.len(), 3, "Should find two files and one subdir");

        assert!(
            entries.clone().iter().filter(|e| e.is_dir()).count() == 1,
            "Should find one directory"
        );
        assert!(
            entries
                .clone()
                .iter()
                .filter(|e| e.is_regular_file())
                .count()
                == 2,
            "Should find two regular files"
        );

        let _ = fs::remove_dir_all(dir_path.as_path());
    }

    #[test]
    fn test_handles_various_tests() {
        // create empty directory
        let tdir = temp_dir().join("NOTAREALPATHLALALA");
        let _ = fs::remove_dir_all(&tdir);
        let _ = fs::create_dir_all(&tdir);

        let dir_entry = DirEntry::new(&tdir).unwrap();

        //PAY ATTENTION TO THE ! MARKS, HARD TO ******** SEE

        assert_eq!(dir_entry.as_bytes(), tdir.as_os_str().as_bytes());
        assert_eq!(dir_entry.as_path(), &tdir);
        assert!(dir_entry.is_dir(), "Should be a directory");
        assert!(
            dir_entry.is_empty(),
            "Directory should be empty {}",
            dir_entry.as_path().display()
        );
        assert!(dir_entry.exists(), "Directory should exist");
        assert!(dir_entry.is_readable(), "Directory should be readable");
        assert!(dir_entry.is_writable(), "Directory should be writable");
        assert!(
            !dir_entry.is_executable(),
            "Directory should be not executable"
        );
        assert!(!dir_entry.is_hidden(), "Directory should be not hidden");
        assert!(!dir_entry.is_symlink(), "Directory should be not symlink");
        let _ = fs::remove_dir_all(&tdir);
    }

    #[test]
    fn test_dirname() {
        let temp_dir = std::env::temp_dir();
        let test_dir = temp_dir.join("test_dirname");
        let file_path = test_dir.join("parent/child.txt");

        let _ = std::fs::remove_dir_all(&test_dir);

        std::fs::create_dir_all(file_path.parent().unwrap())
            .expect("Failed to create parent directory");
        std::fs::write(&file_path, "test").expect("Failed to create test file");

        assert!(file_path.exists(), "Test file was not created");
        assert!(file_path.is_file(), "Test path is not a file");

        // the actual functionality
        let entry = DirEntry::new(file_path.as_os_str()).expect("Failed to create DirEntry");
        assert_eq!(entry.dirname(), b"parent", "Incorrect directory name");

        std::fs::remove_dir_all(&test_dir).expect("Failed to clean up test directory");

        assert!(!test_dir.exists(), "Test directory was not removed");
    }

    //test iteration in a throw away env
    #[test]
    fn test_basic_iteration() {
        let dir_path = temp_dir().join("THROWAWAYANYTHING");
        let _ = fs::remove_dir_all(&dir_path);
        let _ = fs::create_dir_all(&dir_path);

        // create test files
        let _ = File::create(dir_path.join("file1.txt"));
        let _ = fs::create_dir(dir_path.join("subdir"));

        let dir_entry = DirEntry::new(&dir_path).unwrap();
        let iter = dir_entry.readdir().unwrap();
        let entries: Vec<_> = iter.collect();

        assert_eq!(entries.len(), 2);
        let mut names: Vec<_> = entries
            .iter()
            .map(|e| e.as_os_str().to_string_lossy())
            .collect();
        names.sort();

        assert!(names[0].ends_with("file1.txt"));
        assert!(names[1].ends_with("subdir"));

        let _ = fs::remove_dir_all(dir_path);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn test_basic_iteration_linux_android() {
        let dir_path = temp_dir().join("THROWAWAYANYTHINGLINUXANDROID");
        let _ = fs::remove_dir_all(&dir_path);
        let _ = fs::create_dir_all(&dir_path);

        // create test files
        let _ = File::create(dir_path.join("file1.txt"));
        let _ = fs::create_dir(dir_path.join("subdir"));

        let dir_entry = DirEntry::new(&dir_path).unwrap();
        let iter = dir_entry.getdents().unwrap();
        let entries: Vec<_> = iter.collect();

        assert_eq!(entries.len(), 2);
        let mut names: Vec<_> = entries
            .iter()
            .map(|e| e.as_os_str().to_string_lossy())
            .collect();
        names.sort();

        assert!(names[0].ends_with("file1.txt"));
        assert!(names[1].ends_with("subdir"));

        let _ = fs::remove_dir_all(dir_path);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn test_entries_linux() {
        let dir = temp_dir().join("test_dirlinux");
        let _ = fs::remove_dir(&dir);
        let _ = fs::create_dir_all(&dir);
        let dir_entry = DirEntry::new(&dir).unwrap();
        let iter = dir_entry.getdents().unwrap();
        let entries: Vec<_> = iter.collect();
        let _ = fs::remove_dir_all(&dir);

        assert!(dir_entry.is_dir());
        assert_eq!(entries.len(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_entries() {
        let dir = temp_dir().join("test_dir");
        let _ = fs::remove_dir(&dir);
        let _ = fs::create_dir_all(&dir);
        let dir_entry = DirEntry::new(&dir).unwrap();
        let iter = dir_entry.readdir().unwrap();
        let entries: Vec<_> = iter.collect();
        let _ = fs::remove_dir_all(&dir);

        assert!(dir_entry.is_dir());
        assert_eq!(entries.len(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_realpath() {
        let dir = temp_dir().join("test_dir");
        let _ = fs::create_dir_all(&dir);
        let dir_entry = DirEntry::new(&dir).unwrap().to_full_path().unwrap();
        let iter = dir_entry.readdir().unwrap();
        let entries: Vec<_> = iter.collect();
        let _ = fs::remove_dir_all(&dir);

        assert!(dir_entry.is_dir());
        assert_eq!(entries.len(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_size_filter_from_string() {
        assert_eq!(SizeFilter::from_string("100"), Ok(SizeFilter::Equals(100)));
        assert_eq!(SizeFilter::from_string("+100"), Ok(SizeFilter::Min(100)));
        assert_eq!(SizeFilter::from_string("-100"), Ok(SizeFilter::Max(100)));

        // Test unit parsing
        assert_eq!(SizeFilter::from_string("1k"), Ok(SizeFilter::Equals(1000)));
        assert_eq!(SizeFilter::from_string("1kb"), Ok(SizeFilter::Equals(1000)));
        assert_eq!(SizeFilter::from_string("1ki"), Ok(SizeFilter::Equals(1024)));
        assert_eq!(
            SizeFilter::from_string("1kib"),
            Ok(SizeFilter::Equals(1024))
        );

        assert_eq!(
            SizeFilter::from_string("1m"),
            Ok(SizeFilter::Equals(1_000_000))
        );
        assert_eq!(
            SizeFilter::from_string("1mb"),
            Ok(SizeFilter::Equals(1_000_000))
        );
        assert_eq!(
            SizeFilter::from_string("1mi"),
            Ok(SizeFilter::Equals(1_048_576))
        );
        assert_eq!(
            SizeFilter::from_string("1mib"),
            Ok(SizeFilter::Equals(1_048_576))
        );

        assert_eq!(
            SizeFilter::from_string("1g"),
            Ok(SizeFilter::Equals(1_000_000_000))
        );
        assert_eq!(
            SizeFilter::from_string("1gb"),
            Ok(SizeFilter::Equals(1_000_000_000))
        );
        assert_eq!(
            SizeFilter::from_string("1gi"),
            Ok(SizeFilter::Equals(1_073_741_824))
        );
        assert_eq!(
            SizeFilter::from_string("1gib"),
            Ok(SizeFilter::Equals(1_073_741_824))
        );

        assert_eq!(
            SizeFilter::from_string("1t"),
            Ok(SizeFilter::Equals(1_000_000_000_000))
        );
        assert_eq!(
            SizeFilter::from_string("1tb"),
            Ok(SizeFilter::Equals(1_000_000_000_000))
        );
        assert_eq!(
            SizeFilter::from_string("1ti"),
            Ok(SizeFilter::Equals(1_099_511_627_776))
        );
        assert_eq!(
            SizeFilter::from_string("1tib"),
            Ok(SizeFilter::Equals(1_099_511_627_776))
        );

        assert_eq!(SizeFilter::from_string("+1k"), Ok(SizeFilter::Min(1000)));
        assert_eq!(
            SizeFilter::from_string("-2m"),
            Ok(SizeFilter::Max(2_000_000))
        );

        assert!(SizeFilter::from_string("abc").is_err());
        assert!(SizeFilter::from_string("").is_err());
        assert!(SizeFilter::from_string("1x").is_err()); // Invalid unit
    }

    #[test]
    fn test_size_filter_matches() {
        let temp_dir = temp_dir();

        let file_path = temp_dir.join("size_test.txt");
        let _ = fs::remove_file(&file_path);
        let content = vec![0u8; 100]; // 100 bytes
        fs::write(&file_path, content).unwrap();

        let entry = DirEntry::new(&file_path).unwrap();
        let metadata = std::fs::metadata(entry).unwrap();

        let filter = SizeFilter::Equals(100);
        assert!(filter.is_within_size(metadata.len()));

        let filter = SizeFilter::Min(99);
        assert!(filter.is_within_size(metadata.len()));

        let filter = SizeFilter::Min(100);
        assert!(filter.is_within_size(metadata.len()));

        let filter = SizeFilter::Min(101);
        assert!(!filter.is_within_size(metadata.len()));

        let filter = SizeFilter::Max(101);
        assert!(filter.is_within_size(metadata.len()));

        let filter = SizeFilter::Max(100);
        assert!(filter.is_within_size(metadata.len()));

        let filter = SizeFilter::Max(99);
        assert!(!filter.is_within_size(metadata.len()));

        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_metadata_operations() {
        let temp_dir = temp_dir();
        let file_path = temp_dir.join("meta_test.txt");

        {
            let mut f = File::create(&file_path).unwrap();
            writeln!(f, "test content").unwrap();
        }

        let entry = DirEntry::new(&file_path).unwrap();
        let metadata = std::fs::metadata(entry).unwrap();

        assert!(metadata.is_file());
        assert!(!metadata.is_dir());
        assert!(metadata.len() > 0);
        assert!(metadata.modified().is_ok());

        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_file_types() {
        let dir_path = temp_dir().join("THROW_AWAY_THIS");

        let _ = fs::create_dir_all(&dir_path);

        // Create different file types
        let _ = File::create(dir_path.join("regular.txt"));
        let _ = fs::create_dir(dir_path.join("directory"));

        let _ = symlink("regular.txt", dir_path.join("symlink"));

        let dir_entry = DirEntry::new(&dir_path)
            .expect("if this errors then it's probably a permission issue related to sandboxing");
        let entries: Vec<_> = dir_entry.readdir().unwrap().collect();

        let mut type_counts = std::collections::HashMap::new();
        for entry in entries {
            *type_counts.entry(entry.file_type).or_insert(0) += 1;
            println!(
                "File: {}, Type: {:?}",
                entry.as_os_str().to_string_lossy(),
                entry.file_type
            );
        }

        let _ = fs::remove_dir_all(dir_path);
        assert_eq!(type_counts.get(&FileType::RegularFile).unwrap(), &1);
        assert_eq!(type_counts.get(&FileType::Directory).unwrap(), &1);
        assert_eq!(type_counts.get(&FileType::Symlink).unwrap(), &1);
    }

    #[test]
    fn test_non_recursive_iteration() {
        let top_dir = std::env::temp_dir().join("test_nested");
        let _ = fs::remove_dir_all(&top_dir);
        let sub_dir = top_dir.join("subdir");

        let _ = std::fs::create_dir_all(&sub_dir);
        let _ = std::fs::File::create(top_dir.join("top_file.txt"));
        let _ = std::fs::File::create(sub_dir.join("nested_file.txt"));

        let dir_entry = DirEntry::new(&top_dir).unwrap();
        let entries: Vec<_> = dir_entry.readdir().unwrap().collect();

        let mut names: Vec<_> = entries
            .iter()
            .map(|e| String::from_utf8(e.file_name().to_vec()).unwrap())
            .collect();
        names.sort();

        let _ = std::fs::remove_dir_all(&top_dir);
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "subdir"); // Directory entry
        assert_eq!(names[1], "top_file.txt"); // Top-level file
        // Verify nested file wasn't included
        assert!(!names.contains(&"nested_file.txt".to_string()));
    }

    #[test]
    fn test_file_types_realpath() {
        let dir_path = temp_dir().join("THROW_AWAY");
        let _ = fs::remove_dir_all(&dir_path);
        let _ = fs::create_dir_all(&dir_path);

        // Create different file types
        let _ = File::create(dir_path.join("regular.txt"));
        let _ = fs::create_dir(dir_path.join("directory"));

        let _ = symlink("regular.txt", dir_path.join("symlink"));

        let dir_entry = DirEntry::new(&dir_path).unwrap();
        let entries: Vec<_> = dir_entry.readdir().unwrap().collect();

        let mut type_counts = std::collections::HashMap::new();
        for entry in entries {
            *type_counts.entry(entry.file_type).or_insert(0) += 1;
            println!(
                "File: {}, Type: {:?}",
                entry.as_os_str().to_string_lossy(),
                entry.file_type
            );
        }

        assert_eq!(type_counts.get(&FileType::RegularFile).unwrap(), &1);
        assert_eq!(type_counts.get(&FileType::Directory).unwrap(), &1);
        assert_eq!(type_counts.get(&FileType::Symlink).unwrap(), &1);
        let _ = fs::remove_dir_all(dir_path);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_root_linux() {
        //essentially i had a VERY hard to diagnose issue regarding a segfault searching SPECIFICALLY
        //only from root dir (so not applicable to mac, altho ive never used mac fulltime, i am too poor for that)
        let start_path = "/";
        let pattern: &str = ".";

        let finder = Finder::init(OsString::from(&start_path))
            .pattern(&pattern)
            .keep_hidden(true)
            .build()
            .unwrap();

        let result = finder.traverse().unwrap().into_iter();

        let collected: Vec<_> = result.collect();

        assert!(collected.len() > 3);
        //a fairly arbitirary assert, this is to make sure that the result isnt no-opped away.
        //(basically  trying to avoid the same segfault issue seen previously....)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_root_linux_symlinks() {
        // Quick test for symlink recursion detection

        let start_path: &str = "/";
        let pattern: &str = ".";

        let finder = Finder::init(OsString::from(&start_path))
            .pattern(&pattern)
            .keep_hidden(true)
            .follow_symlinks(true)
            .build()
            .unwrap();

        let result = finder.traverse().unwrap().into_iter();

        let collected: Vec<_> = result.collect();

        assert!(collected.len() > 3);
        //a fairly arbitirary assert, this is to make sure that the result isnt no-opped away.
        //(basically  trying to avoid the same segfault issue seen previously....)
    }

    #[test]
    #[allow(unused)]
    fn test_home() {
        let pattern: &str = ".";
        //let home_dir = std::env::home_dir();

        let home_dir = env_home_dir();
        if let Some(ref hd) = home_dir {
            let finder = Finder::init(hd.as_os_str())
                .pattern(pattern)
                .keep_hidden(true)
                .build()
                .unwrap();

            let result = finder.traverse().unwrap();

            let collected: Vec<_> = std::hint::black_box(result.collect());
        }
    }

    #[test]
    #[allow(unused)]
    fn test_home_extension() {
        let pattern: &str = ".";

        let home_dir = env_home_dir();
        if let Some(ref hd) = home_dir {
            let finder = Finder::init(hd.as_os_str())
                .pattern(pattern)
                .extension("c")
                .keep_hidden(true)
                .build()
                .unwrap();

            let result = finder.traverse().unwrap();

            let collected: Vec<_> = std::hint::black_box(result.collect());
        }
    }

    #[test]
    #[allow(unused)]
    fn test_home_extension_symlink() {
        let pattern: &str = ".";

        let home_dir = env_home_dir();
        if let Some(ref hd) = home_dir {
            let finder = Finder::init(hd.as_os_str())
                .pattern(pattern)
                .follow_symlinks(true)
                .extension("c")
                .keep_hidden(true)
                .build()
                .unwrap();

            let result = finder.traverse().unwrap();

            let collected: Vec<_> = std::hint::black_box(result.collect());
        }
    }

    #[test]
    #[allow(unused)]
    fn test_home_symlink() {
        let pattern: &str = ".";
        //let home_dir = std::env::home_dir();
        let home_dir = env_home_dir();
        if let Some(ref hd) = home_dir {
            let finder = Finder::init(hd.as_os_str())
                .pattern(pattern)
                .keep_hidden(true)
                .follow_symlinks(true)
                .build()
                .unwrap();

            let result = finder.traverse().unwrap();

            let collected: Vec<_> = std::hint::black_box(result.collect());
        }
    }

    #[test]
    #[allow(unused)]
    fn test_home_nonhidden() {
        let pattern: &str = ".";
        //let home_dir = std::env::home_dir(); //deprecation shit.

        let home_dir = env_home_dir();
        if let Some(ref hd) = home_dir {
            let finder = Finder::init(hd.as_os_str())
                .pattern(pattern)
                .keep_hidden(false)
                .build()
                .unwrap();

            let result = finder.traverse().unwrap();

            let collected: Vec<_> = std::hint::black_box(result.collect());
        }
    }

    #[test]
    fn test_path_construction() {
        let dir = temp_dir().join("test_pathXXX");
        let _ = fs::create_dir_all(&dir);

        let dir_entry = DirEntry::new(&dir).unwrap();

        let _ = File::create(dir.join("regular.txt"));
        let entries: Vec<_> = dir_entry.readdir().unwrap().collect();
        assert_eq!(entries.len(), 1);

        let v = entries[0]
            .as_os_str()
            .to_string_lossy()
            .contains("regular.txt");

        let _ = std::fs::remove_dir_all(dir);
        assert!(v);
    }

    #[test]
    fn test_filedes_readdir() {
        let dir = temp_dir().join("test_filedes_readdir");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);

        let dir_entry = DirEntry::new(&dir).unwrap();

        let _ = File::create(dir.join("regular.txt"));
        let entries = dir_entry.readdir().unwrap();
        let file_des = entries.dirfd();
        assert!(file_des.is_open());
        let entries_collected: Vec<_> = entries.collect();

        assert_eq!(entries_collected.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn test_filedes_getdents() {
        let dir = temp_dir().join("test_filedes_getdents");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);

        let dir_entry = DirEntry::new(&dir).unwrap();

        let _ = File::create(dir.join("regular.txt"));
        let entries = dir_entry.getdents().unwrap();
        let file_des = entries.dirfd();
        assert!(file_des.is_open());
        let entries_collected: Vec<_> = entries.collect();

        assert_eq!(entries_collected.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_error_handling() {
        let use_path: &str = "/non/existent/pathjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj";

        let std_path = Path::new(use_path);
        if !std_path.exists() {
            let non_existent = DirEntry::new(std_path.as_os_str());
            assert!(non_existent.is_err(), "ok, stop being an ass")
        };
    }

    #[test]
    fn test_time_filter_from_string_basic() {
        // Test basic parsing with different units
        assert!(TimeFilter::from_string("1s").is_ok());
        assert!(TimeFilter::from_string("30m").is_ok());
        assert!(TimeFilter::from_string("1h").is_ok());
        assert!(TimeFilter::from_string("2d").is_ok());
        assert!(TimeFilter::from_string("1w").is_ok());
        assert!(TimeFilter::from_string("1y").is_ok());

        // Test with prefixes
        assert!(TimeFilter::from_string("-1h").is_ok()); // Within last hour
        assert!(TimeFilter::from_string("+2d").is_ok()); // Older than 2 days

        // Test between format
        assert!(TimeFilter::from_string("2d..1d").is_ok());
        assert!(TimeFilter::from_string("1w..2d").is_ok());

        // Test invalid formats
        assert!(TimeFilter::from_string("").is_err());
        assert!(TimeFilter::from_string("abc").is_err());
        assert!(TimeFilter::from_string("1x").is_err()); // Invalid unit
    }

    #[test]
    fn test_time_filter_from_string_units() {
        // Test all unit variants
        assert!(TimeFilter::from_string("1sec").is_ok());
        assert!(TimeFilter::from_string("1second").is_ok());
        assert!(TimeFilter::from_string("1seconds").is_ok());

        assert!(TimeFilter::from_string("1min").is_ok());
        assert!(TimeFilter::from_string("1minute").is_ok());
        assert!(TimeFilter::from_string("1minutes").is_ok());

        assert!(TimeFilter::from_string("1hour").is_ok());
        assert!(TimeFilter::from_string("1hours").is_ok());

        assert!(TimeFilter::from_string("1day").is_ok());
        assert!(TimeFilter::from_string("1days").is_ok());

        assert!(TimeFilter::from_string("1week").is_ok());
        assert!(TimeFilter::from_string("1weeks").is_ok());

        assert!(TimeFilter::from_string("1year").is_ok());
        assert!(TimeFilter::from_string("1years").is_ok());
    }

    #[test]
    fn test_time_filter_matches_time() {
        let now = SystemTime::now();
        let one_hour_ago = now - Duration::from_secs(3600);
        let two_hours_ago = now - Duration::from_secs(7200);
        let one_day_ago = now - Duration::from_secs(86400);

        // Test After (files newer than cutoff)
        let filter = TimeFilter::from_string("-2h").unwrap(); // Within last 2 hours
        assert!(
            filter.matches_time(one_hour_ago),
            "File from 1 hour ago should match filter for last 2 hours"
        );
        assert!(
            !filter.matches_time(one_day_ago),
            "File from 1 day ago should not match filter for last 2 hours"
        );

        // Test Before (files older than cutoff)
        let filter = TimeFilter::from_string("+2h").unwrap(); // Older than 2 hours
        assert!(
            !filter.matches_time(one_hour_ago),
            "File from 1 hour ago should not match filter for older than 2 hours"
        );
        assert!(
            filter.matches_time(one_day_ago),
            "File from 1 day ago should match filter for older than 2 hours"
        );

        let filter = TimeFilter::from_string("1d..2h").unwrap(); // Between 1 day and 2 hours ago
        assert!(
            filter.matches_time(two_hours_ago),
            "File from 2 hours ago should match between filter"
        );
        assert!(
            !filter.matches_time(one_hour_ago),
            "File from 1 hour ago should not match between filter"
        );
        assert!(
            !filter.matches_time(one_day_ago),
            "File from 1 day ago should not match between filter"
        );
    }

    #[test]
    fn test_time_filter_integration_with_finder() {
        let temp_dir = temp_dir().join("time_filter_integration_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // 2 days ago
        let old_file = temp_dir.join("old_file.txt");
        fs::write(&old_file, "old content").unwrap();
        let two_days_ago = SystemTime::now() - Duration::from_secs(2 * 86400);
        let two_days_ago_secs = two_days_ago.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let old_time = FileTime::from_unix_time(two_days_ago_secs, 0);
        set_file_times(&old_file, old_time, old_time).unwrap();

        // 1 hour ago
        let recent_file = temp_dir.join("recent_file.txt");
        fs::write(&recent_file, "recent content").unwrap();
        let one_hour_ago = SystemTime::now() - Duration::from_secs(3600);
        let one_hour_ago_secs = one_hour_ago.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let recent_time = FileTime::from_unix_time(one_hour_ago_secs, 0);
        set_file_times(&recent_file, recent_time, recent_time).unwrap();

        // last 2 hours
        let filter = TimeFilter::from_string("-2h").unwrap();
        let finder = Finder::init(&temp_dir)
            .filter_by_time(Some(filter))
            .build()
            .unwrap();

        let results: Vec<_> = finder.traverse().unwrap().collect();
        assert_eq!(
            results.len(),
            1,
            "Should find exactly 1 file modified within last 2 hours"
        );
        assert!(
            results[0].file_name() == b"recent_file.txt",
            "Should find the recent file"
        );

        // older than 1 day
        let filter = TimeFilter::from_string("+1d").unwrap();
        let finder = Finder::init(&temp_dir)
            .filter_by_time(Some(filter))
            .build()
            .unwrap();

        let results: Vec<_> = finder.traverse().unwrap().collect();
        assert_eq!(
            results.len(),
            1,
            "Should find exactly 1 file older than 1 day"
        );
        assert!(
            results[0].file_name() == b"old_file.txt",
            "Should find the old file"
        );

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_time_filter_between_range() {
        let temp_dir = temp_dir().join("time_filter_between_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // modified 12 hours ago (should be in range of 1d..6h)
        let mid_file = temp_dir.join("mid_file.txt");
        fs::write(&mid_file, "mid content").unwrap();
        let twelve_hours_ago = SystemTime::now() - Duration::from_secs(12 * 3600);
        let twelve_hours_ago_secs = twelve_hours_ago
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let mid_time = FileTime::from_unix_time(twelve_hours_ago_secs, 0);
        set_file_times(&mid_file, mid_time, mid_time).unwrap();

        //  (1 hour ago - should NOT be in range)
        let recent_file = temp_dir.join("recent_file.txt");
        fs::write(&recent_file, "recent content").unwrap();
        let one_hour_ago = SystemTime::now() - Duration::from_secs(3600);
        let one_hour_ago_secs = one_hour_ago.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let recent_time = FileTime::from_unix_time(one_hour_ago_secs, 0);
        set_file_times(&recent_file, recent_time, recent_time).unwrap();

        // (2 days ago - should NOT be in range)
        let old_file = temp_dir.join("old_file.txt");
        fs::write(&old_file, "old content").unwrap();
        let two_days_ago = SystemTime::now() - Duration::from_secs(2 * 86400);
        let two_days_ago_secs = two_days_ago.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let old_time = FileTime::from_unix_time(two_days_ago_secs, 0);
        set_file_times(&old_file, old_time, old_time).unwrap();

        // modified between 1 day and 6 hours ago
        let filter = TimeFilter::from_string("1d..6h").unwrap();
        let finder = Finder::init(&temp_dir)
            .filter_by_time(Some(filter))
            .build()
            .unwrap();

        let results: Vec<_> = finder.traverse().unwrap().collect();
        assert_eq!(
            results.len(),
            1,
            "Should find exactly 1 file in the time range"
        );
        assert!(
            results[0].file_name() == b"mid_file.txt",
            "Should find the file modified 12 hours ago"
        );

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_time_filter_edge_cases() {
        let now = SystemTime::now();

        // Test with very recent time (should match "within last X")
        let filter = TimeFilter::from_string("-1s").unwrap();
        // A file from now should match
        assert!(filter.matches_time(now));

        // Test with very old time
        let very_old = UNIX_EPOCH + Duration::from_secs(946684800); // Year 2000
        let filter = TimeFilter::from_string("+1y").unwrap();
        assert!(
            filter.matches_time(very_old),
            "Very old file should match +1y filter"
        );
    }

    #[test]
    fn test_ignore_regex_integration_with_finder() {
        let temp_dir = temp_dir().join("ignore_regex_integration_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("skip_dir")).unwrap();

        fs::write(temp_dir.join("keep.txt"), "keep").unwrap();
        fs::write(temp_dir.join("skip.log"), "ignore by regex").unwrap();
        fs::write(
            temp_dir.join("skip_dir").join("nested.txt"),
            "ignore by regex",
        )
        .unwrap();

        let finder = Finder::init(&temp_dir)
            .ignore_patterns(vec![String::from("skip")])
            .build()
            .unwrap();

        let mut file_names: Vec<Vec<u8>> = finder
            .traverse()
            .unwrap()
            .filter(|entry| entry.is_regular_file())
            .map(|entry| entry.file_name().to_vec())
            .collect();
        file_names.sort();

        assert_eq!(file_names, vec![b"keep.txt".to_vec()]);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_ignore_glob_integration_with_finder() {
        let temp_dir = temp_dir().join("ignore_glob_integration_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("nested")).unwrap();

        fs::write(temp_dir.join("keep.rs"), "keep").unwrap();
        fs::write(temp_dir.join("drop.tmp"), "ignore by glob").unwrap();
        fs::write(temp_dir.join("nested").join("drop2.tmp"), "ignore by glob").unwrap();

        let finder = Finder::init(&temp_dir)
            .ignore_glob_patterns(vec![String::from("**/*.tmp")])
            .build()
            .unwrap();

        let mut file_names: Vec<Vec<u8>> = finder
            .traverse()
            .unwrap()
            .filter(|entry| entry.is_regular_file())
            .map(|entry| entry.file_name().to_vec())
            .collect();
        file_names.sort();

        assert_eq!(file_names, vec![b"keep.rs".to_vec()]);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_ignore_regex_and_glob_combined_integration() {
        let temp_dir = temp_dir().join("ignore_combined_integration_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("nested")).unwrap();

        fs::write(temp_dir.join("keep.txt"), "keep").unwrap();
        fs::write(temp_dir.join("remove_me.txt"), "ignore by regex").unwrap();
        fs::write(
            temp_dir.join("nested").join("artifact.cache"),
            "ignore by glob",
        )
        .unwrap();

        let finder = Finder::init(&temp_dir)
            .ignore_patterns(vec![String::from("remove_me")])
            .ignore_glob_patterns(vec![String::from("**/*.cache")])
            .build()
            .unwrap();

        let mut file_names: Vec<Vec<u8>> = finder
            .traverse()
            .unwrap()
            .filter(|entry| entry.is_regular_file())
            .map(|entry| entry.file_name().to_vec())
            .collect();
        file_names.sort();

        assert_eq!(file_names, vec![b"keep.txt".to_vec()]);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_ignore_invalid_regex_returns_error() {
        let temp_dir = temp_dir().join("ignore_invalid_regex_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let result = Finder::init(&temp_dir)
            .ignore_patterns(vec![String::from("(")])
            .build();

        assert!(matches!(
            result,
            Err(crate::SearchConfigError::RegexError(_))
        ));

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_ignore_invalid_glob_returns_error() {
        let temp_dir = temp_dir().join("ignore_invalid_glob_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let result = Finder::init(&temp_dir)
            .ignore_glob_patterns(vec![String::from("[")])
            .build();

        assert!(matches!(
            result,
            Err(crate::SearchConfigError::GlobToRegexError(_))
        ));

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
