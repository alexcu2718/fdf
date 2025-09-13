#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use crate::memchr_derivations::{find_char_in_word, find_zero_byte_u64};
    use crate::size_filter::*;
    use crate::traits_and_conversions::BytePath;
    use crate::{DirEntry, DirIter, FileType, SlimmerBytes};
    use chrono::{Duration as ChronoDuration, Utc};
    use filetime::{FileTime, set_file_times};
    use std::env::temp_dir;
    use std::fs;
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use crate::utils::modified_unix_time_to_datetime;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    //helper func to save verbosity
    fn as_bytes(path: &std::path::Path) -> &[u8] {
        path.to_str().unwrap().as_bytes()
    }
    #[allow(dead_code)]
    #[repr(C)]
    pub struct Dirent64 {
        d_ino: u64,
        d_off: u64,
        d_reclen: u16,
        d_type: u8,
        d_name: [u8; 256], // from definition in linux, this is really hacked in order to show my fancy-schmancy SWAR works.
    }

    #[test]
    fn check_filenames() {
        let temp_dir = std::env::temp_dir();
        let file_name = "parent_TEST.txt";
        let file_path = temp_dir.as_path().join(file_name);

        let _ = std::fs::File::create(&file_path);

        let entry: DirEntry<Arc<[u8]>> = DirEntry::new(file_path.as_os_str()).unwrap();
        let _ = std::fs::remove_file(&file_path);
        assert_eq!(entry.file_name(), file_name.as_bytes());
    }

    #[test]
    fn modified_time_fails_for_nonexistent_file() {
        let tmp_dir = temp_dir();
        let file_path = tmp_dir.join("nonexistent_file_should_fail.txt");

        let result = as_bytes(&file_path).modified_time();

        assert!(
            result.is_err(),
            "Expected error for nonexistent file, got {:?}",
            result
        );
    }

    #[test]
    fn test_directory_traversal_permissions() {
        let temp_dir = temp_dir().join("traversal_test");
        fs::create_dir_all(&temp_dir).unwrap();

        // no read permission
        let no_read_dir = temp_dir.join("no_read");
        fs::create_dir(&no_read_dir).unwrap();

        let mut perms = fs::metadata(&no_read_dir).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&no_read_dir, perms).unwrap();

        let entry = DirEntry::<Arc<[u8]>>::new(&temp_dir).unwrap();
        let iter = DirIter::new(&entry).unwrap();

        let entries: Vec<_> = iter.collect();

        let mut perms = fs::metadata(&no_read_dir).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&no_read_dir, perms).unwrap();
        //cleanup code

        fs::remove_dir_all(temp_dir).unwrap();
        assert!(entries.len() == 1)
    }

    #[test]
    fn test_find_char_edge_cases() {
        let all_zeros = [0u8; 8];
        assert_eq!(find_char_in_word(b'x', all_zeros), None);

        let all_x = [b'x'; 8];
        assert_eq!(find_char_in_word(b'x', all_x), Some(0));

        let mixed = [b'a', b'b', 0, b'c', b'd', 0, b'e', b'f'];
        assert_eq!(find_char_in_word(0, mixed), Some(2));
    }

    #[test]
    fn test_permission_checks() {
        let temp_dir = temp_dir().join("perm_test");
        fs::create_dir_all(&temp_dir).unwrap();

        let entry = DirEntry::<Arc<[u8]>>::new(&temp_dir).unwrap();

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

        let dt = as_bytes(&file_path)
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

        let first_time = as_bytes(&file_path)
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

        let second_time = as_bytes(&file_path)
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
        let subdir = dir.join("sub");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "data").unwrap();

        let dir_entry = DirEntry::<Arc<[u8]>>::new(&dir).unwrap();
        let entries: Vec<_> = DirIter::new(&dir_entry).unwrap().collect();

        assert_eq!(entries.len(), 1, "Top-level should contain only subdir");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_size_filter_edge_zero_and_large() {
        let file_zero = temp_dir().join("zero_size.txt");
        File::create(&file_zero).unwrap();
        let entry = DirEntry::<Arc<[u8]>>::new(&file_zero).unwrap();
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

        let entry = DirEntry::<Arc<[u8]>>::new(&link).unwrap();
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

        let dt = as_bytes(&file_path)
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
            "File modified_time too far from now: {:?} vs {:?}",
            dt_timestamp,
            now
        );

        // cleanup
        fs::remove_file(file_path).ok();
    }

    #[test]
    fn test_path_methods() {
        let temp_dir = std::env::temp_dir();
        let test_file_path = temp_dir.join("parent/child.txt");
        let test_dir = test_file_path
            .parent()
            .expect("File path should have parent");

        let _ = std::fs::remove_dir_all(test_dir);
        std::fs::create_dir_all(test_dir).expect("Failed to create test directory");
        std::fs::write(&test_file_path, "test").expect("Failed to write test file");
        let entry: DirEntry<Box<[u8]>> =
            DirEntry::new(test_file_path.as_os_str()).expect("Failed to create DirEntry");
        assert_eq!(entry.file_name(), b"child.txt");
        assert_eq!(entry.extension().unwrap(), b"txt");
        assert_eq!(entry.parent(), test_dir.as_os_str().as_bytes());
        let _ = std::fs::remove_dir_all(test_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_dirent_const_time_strlen_optimal_abc() {
        let mut entry = Dirent64 {
            d_ino: 0,
            d_off: 0,
            d_reclen: 24, // Must be multiple of 8, this is 3 * u64
            d_type: 0,
            d_name: [0; 256],
        };

        entry.d_name[0] = b'a';
        entry.d_name[1] = b'b';
        entry.d_name[2] = b'c';
        entry.d_name[3] = 0;
        //god i hacked this sorry
        let len = unsafe {
            crate::utils::dirent_const_time_strlen(std::mem::transmute::<
                *const Dirent64,
                *const libc::dirent64,
            >(&entry))
        };

        assert_eq!(len, 3);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_dirent_const_time_strlen_single_char() {
        let mut entry = Dirent64 {
            d_ino: 0,
            d_off: 0,
            d_reclen: 24,
            d_type: 0,
            d_name: [0; 256],
        };

        entry.d_name[0] = b'x';
        entry.d_name[1] = 0;
        let len = unsafe {
            crate::utils::dirent_const_time_strlen(std::mem::transmute::<
                *const Dirent64,
                *const libc::dirent64,
            >(&entry))
        };

        assert_eq!(len, 1);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_dirent_const_time_strlen_max_aligned() {
        let mut entry = Dirent64 {
            d_ino: 0,
            d_off: 0,
            d_reclen: 32,
            d_type: 0,
            d_name: [0; 256],
        };

        // 7 chars + null terminator = 8 bytes (perfectly aligned)
        let s = b"abcdefg";
        entry.d_name[..s.len()].copy_from_slice(s);
        entry.d_name[s.len()] = 0;

        let len = unsafe {
            crate::utils::dirent_const_time_strlen(std::mem::transmute::<
                *const Dirent64,
                *const libc::dirent64,
            >(&entry))
        };

        assert_eq!(len, 7);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_dirent_const_time_strlen_exactly_buffer() {
        let mut entry = Dirent64 {
            d_ino: 0,
            d_off: 0,
            d_reclen: 256 + 24, //large enough to fit full name
            d_type: 0,
            d_name: [0; 256],
        };

        // create entire buffer with non-null then add null at end
        entry.d_name.fill(b'x');
        entry.d_name[255] = 0;

        let len = unsafe {
            crate::utils::dirent_const_time_strlen(std::mem::transmute::<
                *const Dirent64,
                *const libc::dirent64,
            >(&entry))
        };

        assert_eq!(len, 255);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_getdents() {
        let temp_dir = std::env::temp_dir();
        let dir_path = temp_dir.as_path().join("testdir");
        let _ = std::fs::create_dir(&dir_path);
        //throwing the error incase it already exists the directory already exists

        std::fs::write(dir_path.join("file1.txt"), "test1").unwrap();
        std::fs::write(dir_path.join("file2.txt"), "test2").unwrap();
        let _ = std::fs::create_dir(dir_path.join("subdir")); //.unwrap();

        let dir_entry: DirEntry<Vec<u8>> = DirEntry::new(dir_path.as_os_str()).unwrap();
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
    fn test_find_dot_u64() {
        let x = *b"123.4567";
        assert_eq!(find_char_in_word(b'.', x), Some(3));
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_read_dir() {
        let temp_dir = std::env::temp_dir();
        let dir_path = temp_dir.as_path().join("testdir");
        let _ = std::fs::create_dir(&dir_path);
        //throwing the error because who cares if the directory already exists

        std::fs::write(dir_path.join("file1.txt"), "test1").unwrap();
        std::fs::write(dir_path.join("file2.txt"), "test2").unwrap();
        let _ = std::fs::create_dir(dir_path.join("subdir")); //.unwrap();

        let dir_entry: DirEntry<Vec<u8>> = DirEntry::new(dir_path.as_os_str()).unwrap();
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
            assert_eq!(
                entry.file_name_index() as usize,
                dir_path.as_os_str().len() + 1
            );
        }

        //let _=std::fs::File::
    }

    #[test]
    fn test_find_char_in_u8_not_found() {
        let x = b"12345678";
        assert_eq!(find_char_in_word(b'.', *x), None);
        assert_eq!(find_char_in_word(0, *x), None);
    }

    #[test]
    fn test_find_char_basic() {
        let data = b"12.45678";
        assert_eq!(find_char_in_word(b'.', *data), Some(2));
    }

    #[test]
    fn test_find_char_first_position() {
        let data = b".1245678";
        assert_eq!(find_char_in_word(b'.', *data), Some(0));
    }

    #[test]
    fn test_find_char_last_position() {
        let data = b"6124567.";
        assert_eq!(find_char_in_word(b'.', *data), Some(7));
    }

    #[test]
    fn test_find_char_not_found() {
        let data = [b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8'];
        assert_eq!(find_char_in_word(b'.', data), None);
        assert_eq!(find_char_in_word(0, data), None);
    }

    #[test]
    fn test_find_special_chars() {
        let data = [b' ', b'\t', b'\n', b'\0', b'-', b'_', b'~', b'@'];
        assert_eq!(find_char_in_word(b' ', data), Some(0));
        assert_eq!(find_char_in_word(b'\0', data), Some(3));
        assert_eq!(find_char_in_word(b'@', data), Some(7));
        assert_eq!(find_char_in_word(b'.', data), None);
    }

    #[test]
    fn test_hidden_files() {
        let dir_path = std::env::temp_dir().join("test_hidden");
        let _ = std::fs::create_dir_all(&dir_path);

        let _ = std::fs::File::create(dir_path.join("visible.txt"));
        let _ = std::fs::File::create(dir_path.join(".hidden"));

        let dir_entry = DirEntry::<Arc<[u8]>>::new(&dir_path).unwrap();
        let entries: Vec<_> = DirIter::new(&dir_entry).unwrap().collect();
        let mut names: Vec<_> = entries
            .iter()
            .map(|e| e.file_name().to_string_lossy().into_owned())
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

        let entry = DirEntry::<SlimmerBytes>::new(file_path.as_os_str()).unwrap();

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

        let entry: usize = DirEntry::<SlimmerBytes>::new(file_path.as_os_str())
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

        let file_path = DirEntry::<SlimmerBytes>::new(".")
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

        let dir_entry = DirEntry::<SlimmerBytes>::new(&temp_dir)
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

        let dir_entry = DirEntry::<SlimmerBytes>::new(&dir_path).unwrap();

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
    fn test_handles_various_tests() {
        // create empty directory
        let tdir = temp_dir().join("NOTAREALPATHLALALA");
        let _ = fs::remove_dir_all(&tdir);
        let _ = fs::create_dir_all(&tdir);

        let dir_entry = DirEntry::<Arc<[u8]>>::new(&tdir).unwrap();

        //PAY ATTENTION TO THE ! MARKS, HARD TO ******** SEE
        assert_eq!(
            dir_entry.parent(),
            tdir.parent().unwrap().as_os_str().as_bytes()
        );
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

        // verify operations succeed
        std::fs::create_dir_all(file_path.parent().unwrap())
            .expect("Failed to create parent directory");
        std::fs::write(&file_path, "test").expect("Failed to create test file");

        // check the file was actually created
        assert!(file_path.exists(), "Test file was not created");
        assert!(file_path.is_file(), "Test path is not a file");

        // the actual functionality
        let entry =
            DirEntry::<Arc<[u8]>>::new(file_path.as_os_str()).expect("Failed to create DirEntry");
        assert_eq!(entry.dirname(), b"parent", "Incorrect directory name");

        // verify removal
        std::fs::remove_dir_all(&test_dir).expect("Failed to clean up test directory");

        assert!(!test_dir.exists(), "Test directory was not removed");
    }
    //test iteration in a throw away env
    #[test]
    fn test_basic_iteration() {
        let dir_path = temp_dir().join("THROWAWAYANYTHING");
        let _ = fs::create_dir_all(&dir_path);

        // create test files
        let _ = File::create(dir_path.join("file1.txt"));
        let _ = fs::create_dir(dir_path.join("subdir"));

        let dir_entry = DirEntry::<Arc<[u8]>>::new(&dir_path).unwrap();
        let iter = DirIter::new(&dir_entry).unwrap();
        let entries: Vec<_> = iter.collect();

        assert_eq!(entries.len(), 2);
        let mut names: Vec<_> = entries
            .iter()
            .map(|e| e.path.as_os_str().to_string_lossy())
            .collect();
        names.sort();

        assert!(names[0].ends_with("file1.txt"));
        assert!(names[1].ends_with("subdir"));

        let _ = fs::remove_dir_all(dir_path);
    }

    #[test]
    fn test_entries() {
        let dir = temp_dir().join("test_dir");
        let _ = fs::create_dir_all(&dir);
        let dir_entry = DirEntry::<Arc<[u8]>>::new(&dir).unwrap();
        let iter = DirIter::new(&dir_entry).unwrap();
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
        let dir_entry = DirEntry::<Arc<[u8]>>::new(&dir)
            .unwrap()
            .to_full_path()
            .unwrap();
        let iter = DirIter::new(&dir_entry).unwrap();
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
        let content = vec![0u8; 100]; // 100 bytes
        fs::write(&file_path, content).unwrap();

        let entry = DirEntry::<Arc<[u8]>>::new(&file_path).unwrap();
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

        let entry = DirEntry::<Arc<[u8]>>::new(&file_path).unwrap();
        let metadata = std::fs::metadata(entry).unwrap();

        assert!(metadata.is_file());
        assert!(!metadata.is_dir());
        assert!(metadata.len() > 0);
        assert!(metadata.modified().is_ok());

        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn no_zero_byte() {
        let value = u64::from_le_bytes([1, 1, 1, 1, 1, 1, 1, 1]);
        assert_eq!(find_zero_byte_u64(value), 8);
    }

    #[test]
    fn first_byte_zero() {
        let value = u64::from_le_bytes([0, 1, 1, 1, 1, 1, 1, 1]);
        assert_eq!(find_zero_byte_u64(value), 0);
    }

    #[test]
    fn last_byte_zero() {
        let value = u64::from_le_bytes([1, 1, 1, 1, 1, 1, 1, 0]);
        assert_eq!(find_zero_byte_u64(value), 7);
    }

    #[test]
    fn middle_byte_zero() {
        let value = u64::from_le_bytes([1, 1, 1, 0, 1, 1, 1, 1]);
        assert_eq!(find_zero_byte_u64(value), 3);
    }

    #[test]
    fn multiple_zeros_returns_first() {
        let value = u64::from_le_bytes([0, 1, 0, 1, 0, 1, 0, 1]);
        assert_eq!(find_zero_byte_u64(value), 0);
    }

    #[test]
    fn all_bytes_zero() {
        let value = u64::from_le_bytes([0; 8]);
        assert_eq!(find_zero_byte_u64(value), 0);
    }

    #[test]
    fn single_zero_in_high_bytes() {
        let value = u64::from_le_bytes([1, 1, 1, 1, 1, 1, 0, 1]);
        assert_eq!(find_zero_byte_u64(value), 6);
    }

    #[test]
    fn adjacent_zeros() {
        let value = u64::from_le_bytes([1, 1, 0, 0, 1, 1, 1, 1]);
        assert_eq!(find_zero_byte_u64(value), 2);
    }

    #[test]
    fn zeros_in_lower_half() {
        let value = u64::from_le_bytes([0, 0, 0, 0, 1, 1, 1, 1]);
        assert_eq!(find_zero_byte_u64(value), 0);
    }

    #[test]
    fn zeros_in_upper_half() {
        let value = u64::from_le_bytes([1, 1, 1, 1, 0, 0, 0, 0]);
        assert_eq!(find_zero_byte_u64(value), 4);
    }

    #[test]
    fn test_file_types() {
        let dir_path = temp_dir().join("THROW_AWAY_THIS");

        let _ = fs::create_dir_all(&dir_path);

        // Create different file types
        let _ = File::create(dir_path.join("regular.txt"));
        let _ = fs::create_dir(dir_path.join("directory"));

        let _ = symlink("regular.txt", dir_path.join("symlink"));

        let dir_entry = DirEntry::<Arc<[u8]>>::new(&dir_path)
            .expect("if this errors then it's probably a permission issue related to sandboxing");
        let entries: Vec<_> = DirIter::new(&dir_entry).unwrap().collect();

        let mut type_counts = std::collections::HashMap::new();
        for entry in entries {
            *type_counts.entry(entry.file_type).or_insert(0) += 1;
            println!(
                "File: {}, Type: {:?}",
                entry.path.as_os_str().to_string_lossy(),
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
        let sub_dir = top_dir.join("subdir");

        let _ = std::fs::create_dir_all(&sub_dir);
        let _ = std::fs::File::create(top_dir.join("top_file.txt"));
        let _ = std::fs::File::create(sub_dir.join("nested_file.txt"));

        let dir_entry = DirEntry::<Arc<[u8]>>::new(&top_dir).unwrap();
        let entries: Vec<_> = DirIter::new(&dir_entry).unwrap().collect();

        let mut names: Vec<_> = entries
            .iter()
            .map(|e| e.file_name().to_string_lossy().into_owned())
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

        let dir_entry = DirEntry::<Arc<[u8]>>::new(&dir_path).unwrap();
        let entries: Vec<_> = DirIter::new(&dir_entry).unwrap().collect();

        let mut type_counts = std::collections::HashMap::new();
        for entry in entries {
            *type_counts.entry(entry.file_type).or_insert(0) += 1;
            println!(
                "File: {}, Type: {:?}",
                entry.path.as_os_str().to_string_lossy(),
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
        use crate::Finder;
        let start_path: &[u8] = b"/";
        let pattern: &str = ".";

        let finder: Finder<SlimmerBytes> = Finder::init(start_path.as_os_str(), &pattern)
            .keep_hidden(true)
            .keep_dirs(true)
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
        use crate::Finder;
        let pattern: &str = ".";
        let home_dir = std::env::home_dir();

        if home_dir.is_some() {
            let finder: Finder<SlimmerBytes> =
                Finder::init(home_dir.unwrap().as_os_str(), &pattern)
                    .keep_hidden(true)
                    .keep_dirs(true)
                    .build()
                    .unwrap();

            let result = finder.traverse().unwrap().into_iter();

            let collected: Vec<_> = std::hint::black_box(result.collect());
        }
    }

    #[test]
    #[allow(unused)]
    fn test_home_nonhidden() {
        use crate::Finder;
        let pattern: &str = ".";
        let home_dir = std::env::home_dir();

        if home_dir.is_some() {
            let finder: Finder<SlimmerBytes> =
                Finder::init(home_dir.unwrap().as_os_str(), &pattern)
                    .keep_hidden(false)
                    .keep_dirs(true)
                    .build()
                    .unwrap();

            let result = finder.traverse().unwrap().into_iter();

            let collected: Vec<_> = std::hint::black_box(result.collect());
        }
    }

    #[test]
    fn test_cstr() {
        let test_bytes = b"randopath";
        let c_str_test: *const u8 = unsafe { cstr!(test_bytes) };
        assert!(
            !c_str_test.is_null(),
            "this should never return a null pointer if it's under {}",
            crate::LOCAL_PATH_MAX
        )
        //well, it'll not pass the test anyway.
    }

    #[test]
    fn test_cstr_n() {
        let test_bytes = b"randopathmpathinlength";
        const SIZE_OF_PATH: usize = 23; //this would panic if it was any bigger
        let c_str_test: *const u8 = unsafe { cstr!(test_bytes, SIZE_OF_PATH) };
        assert!(
            !c_str_test.is_null(),
            "this should never return a null pointer if it's under {}",
            SIZE_OF_PATH
        )
        //well, it'll not pass the test anyway.
    }

    #[test]
    fn test_path_construction() {
        let dir = temp_dir().join("test_pathXXX");
        let _ = fs::create_dir_all(&dir);

        let dir_entry = DirEntry::<Arc<[u8]>>::new(&dir).unwrap();

        let _ = File::create(dir.join("regular.txt"));
        let entries: Vec<_> = DirIter::new(&dir_entry).unwrap().collect();
        assert_eq!(entries.len(), 1);

        let v = entries[0]
            .path
            .as_os_str()
            .to_string_lossy()
            .contains("regular.txt");

        let _ = std::fs::remove_dir_all(dir);
        assert!(v);
    }

    #[test]
    fn test_error_handling() {
        let use_path:&[u8]= b"/non/existent/pathjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj";

        let std_path = Path::new(use_path.as_os_str());
        if std_path.exists() {
            let non_existent = DirEntry::<Arc<[u8]>>::new(use_path.as_os_str());
            assert!(non_existent.is_err(), "ok, stop being an ass")
        };
    }
}
