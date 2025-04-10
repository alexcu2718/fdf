#[cfg(test)]
mod tests {
    use crate::{debug_print, ToOsStr};
    use crate::{DirEntry, DirIter, FileType};
    use std::env::temp_dir;
    use std::fs;
    use std::fs::File;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

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
    fn test_path_methods() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.as_path().join("parent/child.txt");
        let _ = std::fs::remove_dir_all(file_path.parent().unwrap());
        let _ = std::fs::create_dir_all(file_path.parent().unwrap());
        let _ = std::fs::write(&file_path, "test");

        let entry = DirEntry::new(file_path.as_os_str()).unwrap();
        assert_eq!(entry.file_name(), b"child.txt");
        assert_eq!(entry.extension().unwrap(), b"txt");
        assert_eq!(
            entry.parent(),
            file_path.parent().unwrap().as_os_str().as_bytes()
        );
    }

    #[test]
    fn test_read_dir() {
        let temp_dir = std::env::temp_dir();
        let dir_path = temp_dir.as_path().join("testdir");
        let _ = std::fs::create_dir(&dir_path);
        //throwing the error because of the directory already exists

        std::fs::write(dir_path.join("file1.txt"), "test1").unwrap();
        std::fs::write(dir_path.join("file2.txt"), "test2").unwrap();
        let _ = std::fs::create_dir(dir_path.join("subdir")); //.unwrap();

        let dir_entry = DirEntry::new(dir_path.as_os_str()).unwrap();
        let entries = dir_entry.read_dir().unwrap();

        let mut names: Vec<_> = entries.iter().map(|e| e.file_name().to_vec()).collect();
        let _ = std::fs::remove_dir_all(&dir_path).unwrap();
        assert_eq!(entries.len(), 3);

        names.sort();
        assert_eq!(
            names,
            vec![
                b"file1.txt".to_vec(),
                b"file2.txt".to_vec(),
                b"subdir".to_vec()
            ]
        );

        for entry in entries {
            assert_eq!(entry.depth(), 1);
            assert_eq!(entry.base_len() as usize, dir_path.as_os_str().len() + 1);
        }
    }

    #[test]
    fn test_hidden_files() {
        let temp_dir = std::env::temp_dir();
        let hidden_file = temp_dir.as_path().join(".hidden");
        std::fs::write(&hidden_file, "").unwrap();

        let entry = DirEntry::new(hidden_file.as_os_str()).unwrap();
        assert!(entry.is_hidden());

        let non_hidden = temp_dir.as_path().join("visible");
        std::fs::write(&non_hidden, "").unwrap();
        let entry = DirEntry::new(non_hidden.as_os_str()).unwrap();
        assert!(!entry.is_hidden());
    }

    #[test]
    fn filename_test() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.as_path().join("testfile.txt");
        std::fs::write(&file_path, "test").unwrap();

        let entry = DirEntry::new(file_path.as_os_str()).unwrap();
        assert_eq!(entry.file_name(), b"testfile.txt");
    }
    #[test]
    fn base_len_test() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.as_path().join("testfile.txt");
        std::fs::write(&file_path, "test").unwrap();

        let entry: u8 = DirEntry::new(file_path.as_os_str()).unwrap().base_len();
        let std_entry: u8 = (std::path::Path::new(file_path.as_os_str())
            .parent()
            .unwrap()
            .as_os_str()
            .len()
            + 1) as u8;
        assert_eq!(entry, std_entry);
    }
    #[test]
    fn test_full_path() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir().join("test_full_path");
        std::fs::create_dir_all(&temp_dir).unwrap();

        std::env::set_current_dir(&temp_dir).unwrap();
        let file_path = DirEntry::new(".")?.as_full_path()?;
        debug_print!(&file_path);
        let my_path: Box<[u8]> = file_path.as_bytes().into();

        let my_path_std: std::path::PathBuf = std::path::Path::new(".").canonicalize()?;
        let bytes_std: &[u8] = my_path_std.as_os_str().as_bytes();
        assert_eq!(&*my_path, bytes_std);

        assert_eq!(file_path.is_dir(), my_path_std.is_dir());
        Ok(())
    }
    #[test]
    fn test_from_bytes() -> Result<(), Box<dyn std::error::Error>> {
        //this is a mess of code but works lol to demonstrate infallibility(or idealllllllyyyyyyyyy...(ik its not))
        let temp_dir = std::env::temp_dir().join("test_full_path");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);

        //  test file structure
        let test_file = temp_dir.join("test_file.txt");
        let _ = std::fs::write(&test_file, "test content");

        let test_file_canon = PathBuf::from(test_file).canonicalize().unwrap();
        let test_file_canon = test_file_canon.as_os_str().as_bytes();

        // directory entry for temp dir
        let dir_entry = DirEntry::new(&temp_dir)?;
        let canonical_path = temp_dir.canonicalize()?;

        // convert to bytes for most accurate comparison
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
        let mut entries = dir_entry.read_dir()?.into_iter().collect::<Vec<_>>();

        assert!(
            !entries.is_empty(),
            "Should find at least the directory itself"
        );

        let first_entry = entries.pop().unwrap();
        assert_eq!(
            first_entry.as_bytes(),
            test_file_canon,
            "Directory entry should match canonical path"
        );

        //  file type detection
        assert!(first_entry.is_regular_file(), "should be regular file");
        assert_eq!(
            FileType::from_bytes(first_entry.as_bytes()),
            FileType::RegularFile,
            "File type should be regular"
        );

        let pathcheck = std::path::Path::new(first_entry.to_os_str())
            .canonicalize()
            .unwrap();

        assert!(pathcheck.is_file(), "should be a file");

        // Clean up
        let _ = std::fs::remove_dir_all(temp_dir);
        Ok(())
    }

    #[test]
    fn test_iterator() -> core::result::Result<(), Box<dyn std::error::Error>> {
        // make a unique test directory inside temp_dir
        let unique_id = "fdf_iterator_test";
        let dir_path: PathBuf = temp_dir().join(format!("test_dir_{}", unique_id));
        let _ = fs::remove_dir_all(dir_path.as_path());
        let _ = fs::create_dir(dir_path.as_path());

        // create test files and subdirectory
        fs::write(dir_path.join("file1.txt"), "content")?;
        fs::write(dir_path.join("file2.txt"), "content")?;
        fs::create_dir(dir_path.join("subdir"))?;

        // lean up automatically

        // init a DirEntry for testing
        let dir_entry = DirEntry::new(&dir_path)?;

        // get iterator
        let mut iter = dir_entry.as_iter()?;

        // collect entries
        let mut entries = Vec::new();
        while let Some(entry) = iter.next() {
            entries.push(entry);
        }

        // verify results

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

        Ok(())
    }

    #[test]
    fn test_handles_various_tests() -> Result<(), Box<dyn std::error::Error>> {
        // create empty directory
        let tdir = temp_dir().join("NOTAREALPATHLALALA");
        let _ = fs::remove_dir_all(&tdir); //delete it first etc, because thi
        let _ = fs::create_dir_all(&tdir);

        let dir_entry = DirEntry::new(&tdir)?;

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
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.as_path().join("parent/child.txt");
        let _ = std::fs::remove_dir_all(file_path.parent().unwrap());
        let _ = std::fs::create_dir_all(file_path.parent().unwrap());
        let _ = std::fs::write(&file_path, "test");

        let entry = DirEntry::new(file_path.as_os_str()).unwrap();

        assert_eq!(entry.dirname(), b"parent");
        let _ = std::fs::remove_dir_all(file_path.parent().unwrap()).unwrap();
    }
    #[test]
    fn test_basic_iteration() {
        let dir_path = temp_dir().join("THROWAWAYANYTHING");
        let _ = fs::create_dir_all(&dir_path);

        // create test files
        let _ = File::create(dir_path.join("file1.txt"));
        let _ = fs::create_dir(dir_path.join("subdir"));

        let dir_entry = DirEntry::new(&dir_path).unwrap();
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
        let dir_entry = DirEntry::new(&dir).unwrap();
        let iter = DirIter::new(&dir_entry).unwrap();
        let entries: Vec<_> = iter.collect();
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(dir_entry.is_dir(), true);
        assert_eq!(entries.len(), 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_file_types() {
        let dir_path = temp_dir().join("THROW_AWAY");

        let _ = fs::create_dir_all(&dir_path);

        // Create different file types
        let _ = File::create(dir_path.join("regular.txt"));
        let _ = fs::create_dir(dir_path.join("directory"));

        let _ = symlink("regular.txt", dir_path.join("symlink"));

        let dir_entry = DirEntry::new(&dir_path).unwrap();
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
    fn test_path_construction() {
        let dir = temp_dir().join("test_pathXXX");
        let _ = fs::create_dir_all(&dir);

        let dir_entry = DirEntry::new(&dir).unwrap();

        let _ = File::create(&dir.join("regular.txt"));
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
        let non_existent = DirEntry::new("/non/existent/path");
        assert!(non_existent.is_err());
    }
}
