#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use crate::memchr_derivations::find_zero_byte_u64;
    use crate::traits_and_conversions::BytePath;
    use crate::{DirEntry, DirIter, FileType, SlimmerBytes};
    use std::env::temp_dir;
    use std::fs;
    use std::fs::File;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    #[repr(C)]
    #[allow(dead_code)] //only relevant for linux
    pub struct Dirent64 {
        d_ino: u64,
        d_off: u64,
        d_reclen: u16,
        d_type: u8,
        d_name: [u8; 256], // typical max length
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
    fn test_path_methods() {
        // Setup test directory and file
        let temp_dir = std::env::temp_dir();
        let test_file_path = temp_dir.join("parent/child.txt");
        let test_dir = test_file_path
            .parent()
            .expect("File path should have parent");

        // Clean up from previous if errored
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
    fn test_read_dir() {
        let temp_dir = std::env::temp_dir();
        let dir_path = temp_dir.as_path().join("testdir");
        let _ = std::fs::create_dir(&dir_path);
        //throwing the error because of the directory already exists

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
    #[cfg(not(target_os = "linux"))]
    fn test_read_dir() {
        let temp_dir = std::env::temp_dir();
        let dir_path = temp_dir.as_path().join("testdir");
        let _ = std::fs::create_dir(&dir_path);
        //throwing the error because of the directory already exists

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
    fn test_hidden_files() {
        let dir_path = std::env::temp_dir().join("test_hidden");
        let _ = std::fs::create_dir_all(&dir_path);

        // create visible and hidden files
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
        let x = std::fs::remove_file(&file_path).is_ok(); //have to check the result to avoid no-op 
        assert!(x, "File should be removed successfully");
        let _ = std::fs::remove_dir_all(&new_dir);
        //assert!(y.is_ok(), "Directory should be removed successfully");
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
    fn test_full_path() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join("test_full_path");

        let _ = std::fs::remove_dir_all(&temp_dir);
        // delete it first etc, because this is a test
        let _ = std::fs::create_dir_all(&temp_dir);

        let _ = std::env::set_current_dir(&temp_dir); //.unwrap();

        let file_path = DirEntry::<SlimmerBytes>::new(".")?.to_full_path()?;

        let my_path: Box<[u8]> = file_path.as_bytes().into();

        let my_path_std: std::path::PathBuf = std::path::Path::new(".").canonicalize()?;
        let bytes_std: &[u8] = my_path_std.as_os_str().as_bytes();
        assert_eq!(&*my_path, bytes_std);

        assert_eq!(file_path.is_dir(), my_path_std.is_dir());

        let _ = std::fs::remove_dir_all(&temp_dir);
        Ok(())
    }
    #[test]
    #[cfg(not(target_os = "macos"))] //enable this test on macos and see why ive disabled it. **** stupid
    fn test_from_bytes() -> Result<(), Box<dyn std::error::Error>> {
        //this is a mess of code but works lol to demonstrate infallibility(or idealllllllyyyyyyyyy...(ik its not))
        // Create a unique temp directory for this test
        let temp_dir = std::env::temp_dir().join("test_full_path_fdf");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Set up test file
        let test_file = temp_dir.join("test_file_fdf.txt");
        std::fs::write(&test_file, "test content")?;

        // Get canonical paths for comparison
        let test_file_canon = std::fs::canonicalize(&test_file)?;
        let test_file_bytes = test_file_canon.as_os_str().as_bytes();

        // Test directory entry

        let dir_entry = DirEntry::<SlimmerBytes>::new(&temp_dir)?
            .to_full_path()
            .unwrap();

        let canonical_path = temp_dir.canonicalize()?;

        // Compare paths at byte level
        let dir_bytes = dir_entry.as_os_str();
        let canonical_bytes = canonical_path.as_os_str();
        dbg!(&dir_bytes);
        dbg!(&canonical_bytes);

        // verify path conversions in bytes(i want to make sure every byte is right.)
        assert_eq!(
            dir_bytes.as_bytes(),
            canonical_bytes.as_bytes(),
            "Path bytes should matchh"
        );

        // iteration
        let mut entries = dir_entry.readdir()?.into_iter().collect::<Vec<_>>();

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

        Ok(())
    }

    #[test]
    #[clippy::allow(while)]
    fn test_iterator() -> core::result::Result<(), Box<dyn std::error::Error>> {
        // make a unique test directory inside temp_dir
        let unique_id = "fdf_iterator_test";
        let dir_path: PathBuf = temp_dir().join(unique_id);
        let _ = fs::remove_dir_all(dir_path.as_path());
        let _ = fs::create_dir(dir_path.as_path());

        // create test files and subdirectory
        fs::write(dir_path.join("file1.txt"), "content")?;
        fs::write(dir_path.join("file2.txt"), "content")?;
        fs::create_dir(dir_path.join("subdir"))?;

        // lean up automatically

        // init a DirEntry for testing

        let dir_entry = DirEntry::<SlimmerBytes>::new(&dir_path)?;

        // get iterator
        let iter = dir_entry.readdir()?;

        // collect entries
        let mut entries = Vec::new();
        //while let Some(entry) = iter.next() {
        //   entries.push(entry);
        // }

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

        Ok(())
    }

    #[test]
    fn test_handles_various_tests() -> Result<(), Box<dyn std::error::Error>> {
        // create empty directory
        let tdir = temp_dir().join("NOTAREALPATHLALALA");
        let _ = fs::remove_dir_all(&tdir); //delete it first etc, because thi
        let _ = fs::create_dir_all(&tdir);

        let dir_entry = DirEntry::<Arc<[u8]>>::new(&tdir)?;

        //PAY ATTENTION TO THE ! MARKS, HARD TO FUCKING SEE
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

        Ok(())
    }
    #[test]
    fn test_dirname() {
        // use a uniquely named temp directory
        let temp_dir = std::env::temp_dir();
        let test_dir = temp_dir.join("test_dirname");
        let file_path = test_dir.join("parent/child.txt");

        // Cleanup any previous test runs (ignore errors)
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
            .build();

        let result = finder.traverse().unwrap().into_iter();

        let collected: Vec<_> = result.collect();

        assert!(collected.len() > 3);
        //a fairly arbitirary assert, this is to make sure that the result isnt no-opped away.
        //(basically  trying to avoid the same segfault issue seen previously....)
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
        let use_path:&[u8]=            b"/non/existent/pathjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj";
        let std_path = Path::new(use_path.as_os_str());
        assert!(!std_path.exists());
        let non_existent = DirEntry::<Arc<[u8]>>::new(use_path.as_os_str());
        assert!(non_existent.is_err());
    }
}
